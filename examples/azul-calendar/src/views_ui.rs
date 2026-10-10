//! The calendar pane: the reminder and notice lines, the view's header (Previous, Next, its
//! title, Today) and the view - the hours (`timegrid.rs`), the Month (with "+N more" where a
//! day is full), the Schedule View (the day's hours across, a row per calendar) or the List
//! (the coming days). Every event's box is a keyboard stop named by its title and time: a click
//! selects it, a double click or Enter opens it in the editor window.

use azul::{
    dom::{TabIndex, VirtualKeyCode},
    prelude::*,
    shells::ShellEmptyState,
    str::String as AzString,
    widgets::{AlertKind, InfoBar},
};
use chrono::{Datelike, Duration, NaiveDate, NaiveTime};

use crate::{
    chrome, editor_ui, ids, timegrid, views, views::ViewKind, week, CalState, CLIPPED_LINE,
    CLIPPED_TITLE, DAY_PAINT, LINE, NOTICE, NOW_LINE, OTHER_MONTH_PAINT, SECONDARY, SELECTED_RING,
    TODAY_PAINT,
};

/// The month view's day number line and one event line, in px (what "+N more" is counted by).
const MONTH_HEAD_PX: f32 = 22.0;
const MONTH_LINE_PX: f32 = 19.0;
/// What the window's chrome takes above and below the month grid (title row, ribbon, view
/// header, weekday row, status bar), in px: the rest is six weeks of cells.
const MONTH_CHROME_PX: f32 = 270.0;
/// The Schedule View's label column, in px.
const SCHEDULE_LABEL_PX: u32 = 160;

/// The occurrence of `id` on `date` is the selected one.
pub(crate) fn is_selected(s: &CalState, id: &str, date: NaiveDate) -> bool {
    s.selected
        .as_ref()
        .is_some_and(|(sel, day)| sel == id && *day == date)
}

/// An occurrence, for the callbacks on its box.
struct OccurrenceRef {
    app: RefAny,
    id: String,
    date: NaiveDate,
}

/// `dom`, an event's box, made a keyboard stop named `name` that a click selects and a double
/// click or Enter opens.
pub(crate) fn interactive(dom: Dom, app: &RefAny, id: &str, date: NaiveDate, name: String) -> Dom {
    let target = RefAny::new(OccurrenceRef {
        app: app.clone(),
        id: id.to_string(),
        date,
    });
    dom.with_tab_index(TabIndex::Auto)
        .with_accessibility_name(name)
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            target.clone(),
            on_occurrence_click,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::DoubleClick),
            target.clone(),
            on_occurrence_open,
        )
        .with_callback(
            EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
            target,
            on_occurrence_key,
        )
}

/// The calendar pane (`ids::CALENDAR`).
pub(crate) fn calendar_pane(s: &CalState, app: &RefAny, window_height: f32) -> Dom {
    let mut pane = Dom::create_div().with_id(ids::CALENDAR).with_css(
        "display: flex; flex-direction: column; flex-grow: 1; min-width: 0; min-height: 0;",
    );
    if let Some(text) = crate::reminder_text(s) {
        pane.add_child(
            InfoBar::create(text)
                .with_icon("alarm")
                .with_kind(AlertKind::Info)
                .with_action(azul_appkit::l10n::label("azcalendar-dismiss"))
                .with_on_action(app.clone(), crate::on_dismiss_reminder)
                .dom()
                .with_id(ids::REMINDER),
        );
    }
    if s.events.is_empty() {
        pane.add_child(
            InfoBar::create(azul_appkit::l10n::label("azcalendar-empty-calendar"))
            .with_icon("event")
            .with_kind(AlertKind::Info)
            .with_action(azul_appkit::l10n::label("azcalendar-import-more"))
            .with_on_action(app.clone(), chrome::on_open_page)
            .dom()
            .with_id(ids::EMPTY_CALENDAR),
        );
    }
    if !s.notice.is_empty() {
        pane.add_child(
            Dom::create_span_with_text(s.notice.as_str())
                .with_id(ids::NOTICE)
                .with_css(NOTICE),
        );
    }
    pane.add_child(view_header(s, app));
    let view = match s.view {
        ViewKind::Day | ViewKind::WorkWeek | ViewKind::Week => timegrid::time_grid(s, app),
        ViewKind::Month => month_view(s, app, window_height),
        ViewKind::Schedule => schedule_view(s, app),
        ViewKind::Agenda => agenda_view(s, app),
    };
    pane.with_child(view.with_id(ids::view(s.view.name())))
}

/// Previous, Next, the view's title, Today.
fn view_header(s: &CalState, app: &RefAny) -> Dom {
    let icon_button =
        |icon: &str, name: &str, id: AzString, cb: extern "C" fn(RefAny, CallbackInfo) -> Update| {
            Button::create("")
                .with_icon(icon)
                .with_on_click(app.clone(), cb)
                .dom()
                .with_id(id)
                .with_accessibility_name(azul_appkit::l10n::label(name))
                .with_css("margin-right: 4px;")
        };
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; padding: \
             6px 12px; border-bottom: 1px solid {LINE};"
        ))
        .with_child(icon_button(
            "chevron_left",
            "azcalendar-back",
            ids::VIEW_PREV,
            on_previous,
        ))
        .with_child(icon_button(
            "chevron_right",
            "azcalendar-forward",
            ids::VIEW_NEXT,
            on_next,
        ))
        .with_child(
            Dom::create_span_with_text(views::title(s.view, s.anchor))
                .with_id(ids::VIEW_TITLE)
                .with_css("font-size: 18px; margin-left: 8px; flex-grow: 1; min-width: 0;"),
        )
        .with_child(
            Button::create(azul_appkit::l10n::label("azcalendar-today"))
                .with_on_click(app.clone(), chrome::on_today)
                .dom()
                .with_id(ids::VIEW_TODAY),
        )
}

// ==== Month ====

/// The month: the weekday names over six weeks of days (`#month-<yyyymmdd>`), each with its
/// events as lines (time and title, in its calendar's colour) and "+N more" when they do not
/// fit (it opens that day). A day of another month is dimmer; today's number is ringed.
fn month_view(s: &CalState, app: &RefAny, window_height: f32) -> Dom {
    let days = views::days_shown(ViewKind::Month, s.anchor);
    let (first, last) = (days[0], days[days.len() - 1]);
    let occurrences = s.occurrences(first, last);
    let cell_px = (window_height - MONTH_CHROME_PX) / views::MONTH_WEEKS as f32;
    let rows = views::month_cell_rows(cell_px, MONTH_HEAD_PX, MONTH_LINE_PX);
    let month = s.anchor.month();
    let mut names = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: row; flex-shrink: 0; {DAY_PAINT} border-bottom: 1px \
         solid {LINE};"
    ));
    for day in &days[..7] {
        names.add_child(
            Dom::create_div()
                .with_css(format!(
                    "flex-grow: 1; flex-basis: 0px; min-width: 0; padding: 4px 8px; font-size: \
                     12px; {SECONDARY} border-left: 1px solid {LINE};"
                ))
                .with_child(Dom::create_span_with_text(crate::day_text(
                    azul_appkit::l10n::DateStyle::Weekday,
                    *day,
                ))),
        );
    }
    let mut grid = Dom::create_div()
        .with_id(ids::MONTH_GRID)
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0;");
    for week in days.chunks(7) {
        let mut row = Dom::create_div().with_css(format!(
            "display: flex; flex-direction: row; flex-grow: 1; flex-basis: 0px; min-height: 0; \
             border-bottom: 1px solid {LINE};"
        ));
        for day in week {
            row.add_child(month_cell(
                s,
                app,
                *day,
                *day == s.today,
                day.month() == month,
                rows,
                &occurrences,
            ));
        }
        grid.add_child(row);
    }
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0; padding: 0 8px 8px 0;")
        .with_child(names)
        .with_child(grid)
}

fn month_cell(
    s: &CalState,
    app: &RefAny,
    day: NaiveDate,
    today: bool,
    in_month: bool,
    rows: usize,
    occurrences: &[views::Occurrence],
) -> Dom {
    let paint = if today {
        TODAY_PAINT
    } else if in_month {
        DAY_PAINT
    } else {
        OTHER_MONTH_PAINT
    };
    let number_css = if today {
        "font-weight: bold; color: system:accent;"
    } else if in_month {
        ""
    } else {
        SECONDARY
    };
    let label = if day.day() == 1 {
        crate::day_text(azul_appkit::l10n::DateStyle::DayShortMonth, day)
    } else {
        day.day().to_string()
    };
    let target = RefAny::new(DayTarget {
        app: app.clone(),
        day,
    });
    let mut cell = Dom::create_div()
        .with_id(ids::month_day(day))
        .with_css(format!(
            "display: flex; flex-direction: column; flex-grow: 1; flex-basis: 0px; min-width: \
             0; min-height: 0; overflow: hidden; padding: 2px 4px; border-left: 1px solid \
             {LINE}; box-sizing: border-box; {paint}"
        ))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::DoubleClick),
            target.clone(),
            on_day_new_event,
        )
        .with_child(
            Dom::create_span_with_text(label)
                .with_css(format!(
                    "font-size: 12px; height: 18px; flex-shrink: 0; cursor: pointer; {number_css}"
                ))
                .with_accessibility_name(azul_appkit::l10n::t_args(
                    "azcalendar-open-day",
                    &[("day", azul_appkit::l10n::Arg::from(crate::day_text(
                        azul_appkit::l10n::DateStyle::WeekdayDayMonth,
                        day,
                    )))],
                ))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    target.clone(),
                    on_day_open,
                ),
        );
    let items = views::on_day(occurrences, day);
    let (shown, more) = views::month_cell(items.len(), rows);
    for o in items.iter().take(shown) {
        let e = &s.events[o.index];
        let text = if e.all_day {
            e.title.clone()
        } else {
            format!("{} {}", e.start.format("%H:%M"), e.title)
        };
        let selected = is_selected(s, &e.id, o.first);
        cell.add_child(interactive(
            Dom::create_div()
                .with_id(ids::occurrence(&e.id, day))
                .with_css(format!(
                    "{} border-radius: 3px; padding: 0px 4px; margin-top: 1px; height: 17px; \
                     flex-shrink: 0; font-size: 12px; overflow: hidden; {}",
                    s.colour_of(e).bar_css(),
                    if selected { SELECTED_RING } else { "" }
                ))
                .with_child(Dom::create_span_with_text(text).with_css(CLIPPED_LINE)),
            app,
            &e.id,
            o.first,
            format!("{}, {}", e.title, time_label(e)),
        ));
    }
    if more > 0 {
        cell.add_child(
            Dom::create_span_with_text(views::more_label(more))
                .with_id(ids::month_more(day))
                .with_css(
                    "font-size: 12px; color: system:accent; margin-top: 1px; cursor: pointer; \
                     flex-shrink: 0; @theme(flora) { color: system:link; }",
                )
                .with_tab_index(TabIndex::Auto)
                .with_accessibility_name(azul_appkit::l10n::t_args(
                    "azcalendar-more-on",
                    &[
                        ("count", azul_appkit::l10n::Arg::from(more)),
                        (
                            "day",
                            azul_appkit::l10n::Arg::from(crate::day_text(
                                azul_appkit::l10n::DateStyle::WeekdayDayMonth,
                                day,
                            )),
                        ),
                    ],
                ))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    target,
                    on_day_open,
                ),
        );
    }
    cell
}

/// "All day", "09:00 - 10:00".
fn time_label(e: &crate::event::Event) -> String {
    if e.all_day {
        azul_appkit::l10n::t("azcalendar-all-day-lower")
    } else {
        week::time_range(e.start, e.end)
    }
}

// ==== Schedule View ====

/// The day's hours across (`#schedule`), a row per calendar shown, its events as bars; the
/// all-day ones fill a strip at the top of their row; today has the "now" line.
fn schedule_view(s: &CalState, app: &RefAny) -> Dom {
    let day = s.anchor;
    let occurrences = s.occurrences(day, day);
    let mut hours = Dom::create_div()
        .with_css("display: flex; flex-direction: row; flex-grow: 1; min-width: 0;");
    for h in (0..24).step_by(2) {
        hours.add_child(
            Dom::create_div()
                .with_css(format!(
                    "flex-grow: 1; flex-basis: 0px; min-width: 0; font-size: 11px; {SECONDARY} \
                     border-left: 1px solid {LINE}; padding-left: 3px;"
                ))
                .with_child(Dom::create_span_with_text(week::hour_label(h))),
        );
    }
    let mut table = Dom::create_div()
        .with_id(ids::SCHEDULE)
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0; overflow-y: auto; padding: 0 8px 8px 0;")
        .with_child(
            Dom::create_div()
                .with_css(format!(
                    "display: flex; flex-direction: row; flex-shrink: 0; border-bottom: 1px \
                     solid {LINE};"
                ))
                .with_child(Dom::create_div().with_css(format!(
                    "width: {SCHEDULE_LABEL_PX}px; flex-shrink: 0;"
                )))
                .with_child(hours),
        );
    let now = (day == s.today).then(|| week::minute_of_day(chrono::Local::now().time()));
    for calendar in s.calendars.iter().filter(|c| !s.hidden.contains(&c.id)) {
        let mut track = Dom::create_div().with_css(format!(
            "position: relative; flex-grow: 1; min-width: 0; height: 44px; {DAY_PAINT} \
             border-left: 1px solid {LINE};"
        ));
        for o in &occurrences {
            let e = &s.events[o.index];
            if s.calendar_id_of(e) != calendar.id {
                continue;
            }
            let (left, width, top, height) = if e.all_day {
                (0.0, 100.0, 2, 10)
            } else {
                let start = week::minute_of_day(e.start) as f32;
                let end = week::minute_of_day(e.end) as f32;
                (start / 14.4, ((end - start) / 14.4).max(0.6), 14, 28)
            };
            let selected = is_selected(s, &e.id, o.first);
            track.add_child(interactive(
                Dom::create_div()
                    .with_id(ids::occurrence(&e.id, day))
                    .with_css(format!(
                        "position: absolute; left: {left:.3}%; width: {width:.3}%; top: \
                         {top}px; height: {height}px; box-sizing: border-box; border-radius: \
                         3px; padding: 1px 4px; font-size: 11px; overflow: hidden; {} {}",
                        calendar.colour.bar_css(),
                        if selected { SELECTED_RING } else { "" }
                    ))
                    .with_child(
                        Dom::create_span_with_text(e.title.as_str()).with_css(CLIPPED_TITLE),
                    ),
                app,
                &e.id,
                o.first,
                format!("{}, {}", e.title, time_label(e)),
            ));
        }
        if let Some(now) = now {
            track.add_child(Dom::create_div().with_css(format!(
                "position: absolute; top: 0px; height: 100%; width: 2px; left: {:.3}%; \
                 {NOW_LINE}",
                now as f32 / 14.4
            )));
        }
        table.add_child(
            Dom::create_div()
                .with_css(format!(
                    "display: flex; flex-direction: row; flex-shrink: 0; border-bottom: 1px \
                     solid {LINE};"
                ))
                .with_child(
                    Dom::create_div()
                        .with_css(format!(
                            "width: {SCHEDULE_LABEL_PX}px; flex-shrink: 0; display: flex; \
                             flex-direction: row; align-items: center; padding: 0 8px; \
                             box-sizing: border-box;"
                        ))
                        .with_child(Dom::create_div().with_css(format!(
                            "width: 10px; height: 10px; border-radius: 2px; margin-right: 6px; \
                             flex-shrink: 0; {}",
                            calendar.colour.swatch_css()
                        )))
                        .with_child(
                            Dom::create_span_with_text(crate::calendar_name(calendar))
                                .with_css(CLIPPED_LINE),
                        ),
                )
                .with_child(track),
        );
    }
    table
}

// ==== List ====

/// The coming days (`#agenda`): each day with events, its events as rows - time, the
/// calendar's swatch, title, place, and how it repeats.
fn agenda_view(s: &CalState, app: &RefAny) -> Dom {
    let days = views::days_shown(ViewKind::Agenda, s.anchor);
    let (first, last) = (days[0], days[days.len() - 1]);
    let occurrences = s.occurrences(first, last);
    let list = views::agenda(&occurrences, first, last);
    if list.is_empty() {
        return ShellEmptyState::create(azul_appkit::l10n::label("azcalendar-agenda-empty"))
            .with_icon("event_available")
            .with_detail(azul_appkit::l10n::label("azcalendar-agenda-empty-detail"))
            .with_action_label(azul_appkit::l10n::label("azcalendar-new-appointment"))
            .with_on_action(app.clone(), editor_ui::on_new_appointment)
            .dom()
            .with_id(ids::AGENDA);
    }
    let mut out = Dom::create_div()
        .with_id(ids::AGENDA)
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0; overflow-y: auto; padding: 8px 16px;");
    for (day, items) in list {
        out.add_child(
            Dom::create_span_with_text(views::agenda_day_label(day, s.today)).with_css(format!(
                "font-weight: bold; margin-top: 12px; padding-bottom: 4px; border-bottom: 1px \
                 solid {LINE};"
            )),
        );
        for o in items {
            let e = &s.events[o.index];
            let mut text = Dom::create_div()
                .with_css("display: flex; flex-direction: column; flex-grow: 1; min-width: 0;")
                .with_child(Dom::create_span_with_text(e.title.as_str()).with_css(CLIPPED_TITLE));
            let mut details: Vec<String> = Vec::new();
            if !e.location.is_empty() {
                details.push(e.location.clone());
            }
            if let Some(rule) = &e.repeat {
                details.push(azul_appkit::l10n::t_said(&rule.description(e.date)));
            }
            if !details.is_empty() {
                text.add_child(
                    Dom::create_span_with_text(details.join(" \u{b7} ")).with_css(CLIPPED_LINE),
                );
            }
            let selected = is_selected(s, &e.id, o.first);
            out.add_child(interactive(
                Dom::create_div()
                    .with_id(ids::occurrence(&e.id, day))
                    .with_css(format!(
                        "display: flex; flex-direction: row; align-items: center; padding: 6px \
                         4px; border-bottom: 1px solid {LINE}; {}",
                        if selected { SELECTED_RING } else { "" }
                    ))
                    .with_child(
                        Dom::create_span_with_text(time_label(e))
                            .with_css(format!("width: 110px; flex-shrink: 0; {SECONDARY}")),
                    )
                    .with_child(Dom::create_div().with_css(format!(
                        "width: 4px; height: 28px; border-radius: 2px; margin-right: 10px; \
                         flex-shrink: 0; {}",
                        s.colour_of(e).swatch_css()
                    )))
                    .with_child(text),
                app,
                &e.id,
                o.first,
                format!("{}, {}", e.title, time_label(e)),
            ));
        }
    }
    out
}

// ==== Callbacks ====

fn step(data: &mut RefAny, by: i32) -> Update {
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let day = views::step(s.view, s.anchor, by);
    s.set_anchor(day);
    Update::RefreshDom
}

extern "C" fn on_previous(mut data: RefAny, _info: CallbackInfo) -> Update {
    step(&mut data, -1)
}

extern "C" fn on_next(mut data: RefAny, _info: CallbackInfo) -> Update {
    step(&mut data, 1)
}

/// A click on an event's box selects that occurrence.
extern "C" fn on_occurrence_click(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, id, date)) = data
        .downcast_ref::<OccurrenceRef>()
        .map(|r| (r.app.clone(), r.id.clone(), r.date))
    else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    if s.selected.as_ref() == Some(&(id.clone(), date)) {
        return Update::DoNothing;
    }
    s.selected = Some((id, date));
    Update::RefreshDom
}

/// A double click on an event's box opens it in the editor window.
extern "C" fn on_occurrence_open(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, id, date)) = data
        .downcast_ref::<OccurrenceRef>()
        .map(|r| (r.app.clone(), r.id.clone(), r.date))
    else {
        return Update::DoNothing;
    };
    editor_ui::open_event(&mut app, &mut info, &id, date)
}

/// Enter (or Space) on a focused event opens it; Delete asks nothing and does nothing (deleting
/// is the editor's).
extern "C" fn on_occurrence_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    if !matches!(
        key,
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter | VirtualKeyCode::Space)
    ) {
        return Update::DoNothing;
    }
    let Some((mut app, id, date)) = data
        .downcast_ref::<OccurrenceRef>()
        .map(|r| (r.app.clone(), r.id.clone(), r.date))
    else {
        return Update::DoNothing;
    };
    info.prevent_default();
    editor_ui::open_event(&mut app, &mut info, &id, date)
}

/// A day of the month view, for its callbacks.
struct DayTarget {
    app: RefAny,
    day: NaiveDate,
}

/// The day's number or its "+N more": that day, in the Day view.
extern "C" fn on_day_open(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, day)) = data
        .downcast_ref::<DayTarget>()
        .map(|r| (r.app.clone(), r.day))
    else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    s.set_view(ViewKind::Day);
    s.set_anchor(day);
    Update::RefreshDom
}

/// A double click on a month day's empty space: a new appointment on it, at 09:00.
extern "C" fn on_day_new_event(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, day)) = data
        .downcast_ref::<DayTarget>()
        .map(|r| (r.app.clone(), r.day))
    else {
        return Update::DoNothing;
    };
    let nine = NaiveTime::from_hms_opt(9, 0, 0).unwrap_or(NaiveTime::MIN);
    editor_ui::open_new_at(
        &mut app,
        &mut info,
        day,
        nine,
        nine + Duration::hours(1),
        false,
    )
}
