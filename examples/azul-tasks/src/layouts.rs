//! The task list pane's other layouts (azul-apps/planning/core/todo.md 2.3 and 2.4), each the
//! same tasks the list shows, laid out another way:
//!
//! - Scheduled as the planned MONTH: six weeks of days (`azul_pim::dates::month_grid`, from the
//!   settings' week start), each day with its open tasks due then (`views::planned_month`); a
//!   task dragged onto another day is due then (`Tasks::reschedule`). Previous / Next / Today
//!   step the month.
//! - A list as its BOARD: To do, Doing, Done (`views::board`), a card per task; a card dragged
//!   onto another column is started, completed or opened again (`Tasks::move_to_column`).
//!
//! The header's "List | Month" / "List | Board" switch (`ids::LAYOUT_SWITCH`) picks the layout.
//! A click on a task selects it as a row's click does (the reading pane shows it); it drags as
//! a row does (`list::on_row_drag_start`), onto the navigation pane too.
//!
//! On stdout: `AZTASKS_LAYOUT <view> <list|month|board>`, `AZTASKS_MONTH <yyyy-mm>`, and the
//! drops' `AZTASKS_DUE <task> <day>` / `AZTASKS_COLUMN <task> <column>` (`state.rs`).

use azul::{
    callbacks::{ButtonOnClickCallbackType, SegmentedOnChangeCallbackType},
    dom::{AttributeType, TabIndex},
    prelude::*,
    str::String as AzString,
    vec::StringVec,
    widgets::{Card, Chip, ChipKind, Segmented, SegmentedState},
};
use chrono::{Datelike, NaiveDate, NaiveDateTime};

use azul_appkit::l10n::{label, t, t_args, Arg, DateStyle};

use crate::{
    ids, list,
    model::{self, Priority, Task},
    state::Tasks,
    views::{self, Column, Smart, View},
};

// ==== Styles (system colours: they follow the mode) ====

const MONTH: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; \
                     padding: 0px 8px 8px 8px;";
const MONTH_BAR: &str = "display: flex; flex-direction: row; align-items: center; gap: 6px; \
                         padding: 4px 8px 8px 8px; flex-shrink: 0;";
const MONTH_TITLE: &str = "font-size: 15px; font-weight: bold; flex-grow: 1;";
const WEEK_ROW: &str = "display: flex; flex-direction: row; flex-grow: 1; flex-basis: 0px; \
                        min-height: 0px; border-bottom: 1px solid system:separator;";
const WEEKDAY_ROW: &str = "display: flex; flex-direction: row; flex-shrink: 0; border-bottom: \
                           1px solid system:separator;";
const WEEKDAY: &str = "flex-grow: 1; flex-basis: 0px; min-width: 0px; padding: 2px 4px; \
                       font-size: 11px; color: system:secondary-text;";
const DAY: &str = "display: flex; flex-direction: column; flex-grow: 1; flex-basis: 0px; \
                   min-width: 0px; min-height: 0px; overflow: hidden; padding: 2px 3px; gap: 2px; \
                   border-left: 1px solid system:separator; box-sizing: border-box;";
const DAY_NUMBER: &str = "font-size: 11px; flex-shrink: 0;";
const DAY_NUMBER_OTHER: &str = "font-size: 11px; flex-shrink: 0; color: system:tertiary-text;";
const DAY_NUMBER_TODAY: &str = "font-size: 11px; flex-shrink: 0; align-self: flex-start; padding: \
                                0px 5px; border-radius: 8px; background: system:accent; color: \
                                system:accent-text;";
const PLANNED: &str = "font-size: 11px; flex-shrink: 0; padding: 1px 4px; border-radius: 3px; \
                       white-space: nowrap; overflow: hidden; text-overflow: ellipsis; \
                       background: system:control-background; cursor: default;";
const PLANNED_SELECTED: &str = "font-size: 11px; flex-shrink: 0; padding: 1px 4px; \
                                border-radius: 3px; white-space: nowrap; overflow: hidden; \
                                text-overflow: ellipsis; background: system:accent; color: \
                                system:accent-text; cursor: default;";
const MORE: &str = "font-size: 11px; flex-shrink: 0; color: system:secondary-text;";

/// How many tasks a day of the month shows before "+N more".
const MONTH_TASKS: usize = 2;

// ==== The switch ====

/// The header's switch for the view shown: "List | Month" on Scheduled, "List | Board" on a
/// list, none elsewhere.
pub(crate) fn switch(s: &Tasks, app: &RefAny) -> Option<Dom> {
    let (other, on) = match &s.view {
        View::Smart(Smart::Scheduled) => ("aztasks-layout-month", s.planned_month),
        View::List(_) => ("aztasks-layout-board", s.board),
        _ => return None,
    };
    Some(
        Segmented::create(StringVec::from(vec![label("aztasks-layout-list"), label(other)]))
        .with_selected_index(usize::from(on))
        .with_on_change(app.clone(), on_layout as SegmentedOnChangeCallbackType)
        .dom()
        .with_id(ids::LAYOUT_SWITCH),
    )
}

/// The pane's body in the layout the view shows, when it is not the list.
pub(crate) fn body(s: &Tasks, app: &RefAny, now: NaiveDateTime) -> Option<Dom> {
    match &s.view {
        View::Smart(Smart::Scheduled) if s.planned_month => Some(month(s, app, now)),
        View::List(id) if s.board => Some(board(s, app, id, now)),
        _ => None,
    }
}

extern "C" fn on_layout(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let on = state.selected_index == 1;
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        let layout = match s.view {
            View::Smart(Smart::Scheduled) => {
                s.planned_month = on;
                if on { "month" } else { "list" }
            }
            View::List(_) => {
                s.board = on;
                if on { "board" } else { "list" }
            }
            _ => return,
        };
        println!("AZTASKS_LAYOUT {} {layout}", s.view.name());
    })
}

// ==== The planned month ====

/// What a day's callbacks carry.
struct DayRef {
    app: RefAny,
    day: NaiveDate,
}

/// The planned month of `s.month`: the bar (Previous, the month, Next, Today), the weekdays,
/// six weeks of days.
fn month(s: &Tasks, app: &RefAny, now: NaiveDateTime) -> Dom {
    let today = now.date();
    let days = azul_pim::dates::month_grid(s.month, s.settings.week_start);
    let cells = views::planned_month(&s.tasks, &days);
    let button = |text: &str, icon: &str, id: AzString, name: &str, cb: ButtonOnClickCallbackType| {
        Button::create(label(text))
            .with_icon(icon)
            .with_on_click(app.clone(), cb)
            .dom()
            .with_id(id)
            .with_accessibility_name(label(name))
    };
    let bar = Dom::create_div()
        .with_css(MONTH_BAR)
        .with_child(button("", "chevron_left", ids::MONTH_PREV, "aztasks-month-previous", on_month_prev))
        .with_child(
            Dom::create_span_with_text(model::said(DateStyle::MonthYear, s.month)).with_css(MONTH_TITLE),
        )
        .with_child(button("", "chevron_right", ids::MONTH_NEXT, "aztasks-month-next", on_month_next))
        .with_child(button("kit-date-today", "today", ids::MONTH_TODAY, "aztasks-month-this", on_month_today));
    let mut weekdays = Dom::create_div().with_css(WEEKDAY_ROW);
    for d in days.iter().take(7) {
        weekdays.add_child(
            Dom::create_span_with_text(t(azul_pim::dates::weekday_short_message_id(d.weekday())))
                .with_css(WEEKDAY),
        );
    }
    let mut grid = Dom::create_div()
        .with_id(ids::PLANNED_MONTH)
        .with_css(MONTH)
        .with_accessibility_name(label("aztasks-planned-month"))
        .with_child(bar)
        .with_child(weekdays);
    for (week, week_cells) in days.chunks(7).zip(cells.chunks(7)) {
        let mut row = Dom::create_div().with_css(WEEK_ROW);
        for (day, tasks) in week.iter().zip(week_cells) {
            row.add_child(day_cell(s, app, *day, tasks, today));
        }
        grid.add_child(row);
    }
    grid
}

/// One day: its number (today's ringed, another month's dim), its first tasks, "+N more"; a
/// task dropped on it is due that day.
fn day_cell(s: &Tasks, app: &RefAny, day: NaiveDate, tasks: &[usize], today: NaiveDate) -> Dom {
    let number_css = if day == today {
        DAY_NUMBER_TODAY
    } else if day.month() == s.month.month() {
        DAY_NUMBER
    } else {
        DAY_NUMBER_OTHER
    };
    let target = || RefAny::new(DayRef { app: app.clone(), day });
    let mut cell = Dom::create_div()
        .with_id(ids::month_day(&model::format_date(day)))
        .with_css(DAY)
        .with_accessibility_name(model::said(DateStyle::WeekdayDayMonth, day))
        .with_child(Dom::create_span_with_text(day.day().to_string()).with_css(number_css));
    cell.add_callback(EventFilter::Hover(HoverEventFilter::DragOver), target(), on_day_drag_over);
    cell.add_callback(EventFilter::Hover(HoverEventFilter::Drop), target(), on_day_drop);
    for &i in tasks.iter().take(MONTH_TASKS) {
        cell.add_child(planned_task(s, app, &s.tasks[i]));
    }
    if tasks.len() > MONTH_TASKS {
        cell.add_child(
            Dom::create_span_with_text(t_args(
                "aztasks-more",
                &[("count", Arg::from(tasks.len() - MONTH_TASKS))],
            ))
            .with_css(MORE),
        );
    }
    cell
}

/// A task in a day: its time and title; a click selects it, it drags as a row does.
fn planned_task(s: &Tasks, app: &RefAny, t: &Task) -> Dom {
    let text = match t.due_time {
        Some(time) => format!("{} {}", model::format_time(time), t.title),
        None => t.title.clone(),
    };
    let css = if s.is_selected(&t.id) { PLANNED_SELECTED } else { PLANNED };
    let mut dom = Dom::create_div()
        .with_class(ids::PLANNED_TASK_CLASS)
        .with_css(css)
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_name(t.title.as_str())
        .with_attribute(AttributeType::draggable(true))
        .with_child(Dom::create_span_with_text(text));
    dom.add_callback(EventFilter::Hover(HoverEventFilter::MouseDown), list::row_ref(app, &t.id), list::on_row_down);
    dom.add_callback(EventFilter::Hover(HoverEventFilter::DragStart), list::row_ref(app, &t.id), list::on_row_drag_start);
    dom
}

/// Steps the month shown by `months` (0: this month).
fn step_month(data: &mut RefAny, info: &mut CallbackInfo, months: Option<i32>) -> Update {
    crate::with_tasks(data, info, |_info, _app, s| {
        s.month = match months {
            Some(n) => azul_pim::dates::add_months_clamped(s.month, n, 1),
            None => crate::state::now().date(),
        };
        println!("AZTASKS_MONTH {}", s.month.format("%Y-%m"));
    })
}

extern "C" fn on_month_prev(mut data: RefAny, mut info: CallbackInfo) -> Update {
    step_month(&mut data, &mut info, Some(-1))
}

extern "C" fn on_month_next(mut data: RefAny, mut info: CallbackInfo) -> Update {
    step_month(&mut data, &mut info, Some(1))
}

extern "C" fn on_month_today(mut data: RefAny, mut info: CallbackInfo) -> Update {
    step_month(&mut data, &mut info, None)
}

extern "C" fn on_day_drag_over(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.accept_drop();
    Update::DoNothing
}

/// A drop on a day: the dropped tasks are due that day.
extern "C" fn on_day_drop(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, day)) = data.downcast_ref::<DayRef>().map(|d| (d.app.clone(), d.day)) else {
        return Update::DoNothing;
    };
    crate::with_tasks(&mut app, &mut info, |_info, _app, s| {
        for id in s.take_dropped() {
            if let Some(i) = s.index_of(&id) {
                s.reschedule(i, day);
            }
        }
    })
}

// ==== The board ====

const BOARD: &str = "display: flex; flex-direction: row; align-items: stretch; flex-grow: 1; \
                     min-height: 0px; gap: 10px; padding: 4px 16px 12px 16px; overflow-x: auto;";
// Under flora (`@theme(flora)` after the flat values): a column is a leaf at flora's 5px, a
// selected card lies on the theme's selection at 3px, a priority is the clay stone.
const COLUMN: &str = "display: flex; flex-direction: column; flex-grow: 1; flex-basis: 0px; \
                      min-width: 160px; min-height: 0px; gap: 6px; padding: 8px; border-radius: \
                      8px; background: system:control-background; overflow-y: auto; \
                      @theme(flora) { border-radius: 5px; }";
const COLUMN_HEAD: &str = "display: flex; flex-direction: row; align-items: center; gap: 6px; \
                           flex-shrink: 0; padding: 0px 2px 4px 2px;";
const COLUMN_TITLE: &str = "font-size: 12px; font-weight: bold; letter-spacing: 0.5px;";
const COLUMN_COUNT: &str = "font-size: 12px; color: system:secondary-text;";
const COLUMN_EMPTY: &str = "font-size: 12px; color: system:tertiary-text; padding: 8px 2px;";
const CARD_BODY: &str = "display: flex; flex-direction: column; gap: 4px; min-width: 0px;";
const CARD_BODY_SELECTED: &str = "display: flex; flex-direction: column; gap: 4px; min-width: \
                                  0px; margin: -4px; padding: 4px; border-radius: 4px; \
                                  background: var(--az-accent-soft, #e1e6e1); @media \
                                  (prefers-color-scheme: dark) { background: \
                                  var(--az-accent-soft, #2f4c39); } @theme(flora) { \
                                  border-radius: 3px; background: \
                                  system:selection-background; }";
const CARD_TITLE: &str = "font-size: 13px;";
const CARD_TITLE_DONE: &str = "font-size: 13px; color: system:secondary-text; text-decoration: \
                               line-through;";
const CARD_META: &str = "display: flex; flex-direction: row; flex-wrap: wrap; align-items: \
                         center; gap: 6px; font-size: 11px; color: system:secondary-text;";
const CARD_PRIORITY: &str = "font-weight: bold; color: #c25e00; @media (prefers-color-scheme: \
                             dark) { color: #ffb366; } @theme(flora) { color: #7E4A42; @media \
                             (prefers-color-scheme: dark) { color: #B3837A; } }";

/// What a column's callbacks carry.
struct ColumnRef {
    app: RefAny,
    column: Column,
}

/// The board of list `list`: a column per [`Column`], a card per task.
fn board(s: &Tasks, app: &RefAny, list: &str, now: NaiveDateTime) -> Dom {
    let columns = views::board(&s.tasks, list);
    let mut out = Dom::create_div()
        .with_id(ids::BOARD)
        .with_css(BOARD)
        .with_accessibility_name(t_args("aztasks-board-of", &[("list", Arg::from(s.list_name(list)))]));
    for (column, tasks) in Column::ALL.into_iter().zip(columns.iter()) {
        out.add_child(column_dom(s, app, column, tasks, now));
    }
    out
}

/// One column: its title and count, its cards (or what a drop does); a card dropped on it
/// moves there.
fn column_dom(s: &Tasks, app: &RefAny, column: Column, tasks: &[usize], now: NaiveDateTime) -> Dom {
    let target = || RefAny::new(ColumnRef { app: app.clone(), column });
    let mut out = Dom::create_div()
        .with_id(ids::board_column(column.key()))
        .with_css(COLUMN)
        .with_accessibility_name(column.label())
        .with_child(
            Dom::create_div()
                .with_css(COLUMN_HEAD)
                .with_child(Dom::create_span_with_text(column.label().to_uppercase()).with_css(COLUMN_TITLE))
                .with_child(Dom::create_span_with_text(tasks.len().to_string()).with_css(COLUMN_COUNT)),
        );
    out.add_callback(EventFilter::Hover(HoverEventFilter::DragOver), target(), on_column_drag_over);
    out.add_callback(EventFilter::Hover(HoverEventFilter::Drop), target(), on_column_drop);
    if tasks.is_empty() {
        let hint = match column {
            Column::ToDo => "aztasks-board-empty-to-do",
            Column::Doing => "aztasks-board-empty-doing",
            Column::Done => "aztasks-board-empty-done",
        };
        out.add_child(Dom::create_span_with_text(label(hint)).with_css(COLUMN_EMPTY));
    }
    for &i in tasks {
        out.add_child(card(s, app, &s.tasks[i], now));
    }
    out
}

/// A task's card: its priority, title and flag; its due chip, steps and tags. A click selects
/// it, it drags as a row does.
fn card(s: &Tasks, app: &RefAny, t: &Task, now: NaiveDateTime) -> Dom {
    let today = now.date();
    let mut title = Dom::create_div().with_css(CARD_META);
    if t.priority != Priority::None {
        title.add_child(Dom::create_span_with_text(t.priority.mark()).with_css(CARD_PRIORITY));
    }
    title.add_child(
        Dom::create_span_with_text(t.title.as_str())
            .with_css(if t.is_done() { CARD_TITLE_DONE } else { CARD_TITLE }),
    );
    if t.flagged {
        title.add_child(Dom::create_icon("flag").with_accessibility_name(label("aztasks-smart-flagged")));
    }
    let mut meta = Dom::create_div().with_css(CARD_META);
    let mut any_meta = false;
    if let Some(done) = t.completed {
        meta.add_child(Dom::create_span_with_text(t_args(
            "aztasks-completed-on",
            &[("day", Arg::from(model::day_label(done.date(), today)))],
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
        meta.add_child(Chip::with_kind(label, kind).dom());
        any_meta = true;
    }
    if let Some((done, total)) = t.subtask_progress() {
        meta.add_child(Dom::create_span_with_text(format!("\u{2611} {done}/{total}")));
        any_meta = true;
    }
    for tag in &t.tags {
        meta.add_child(Dom::create_span_with_text(format!("#{tag}")));
        any_meta = true;
    }
    let body_css = if s.is_selected(&t.id) { CARD_BODY_SELECTED } else { CARD_BODY };
    let mut body = Dom::create_div().with_css(body_css).with_child(title);
    if any_meta {
        body.add_child(meta);
    }
    let mut dom = Card::create(body)
        .dom()
        .with_id(ids::board_card(&t.id))
        .with_class(ids::BOARD_CARD_CLASS)
        .with_css("flex-shrink: 0;")
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_name(t.title.as_str())
        .with_attribute(AttributeType::draggable(true));
    dom.add_callback(EventFilter::Hover(HoverEventFilter::MouseDown), list::row_ref(app, &t.id), list::on_row_down);
    dom.add_callback(EventFilter::Hover(HoverEventFilter::DragStart), list::row_ref(app, &t.id), list::on_row_drag_start);
    dom
}

extern "C" fn on_column_drag_over(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.accept_drop();
    Update::DoNothing
}

/// A drop on a column: the dropped tasks of this list move there.
extern "C" fn on_column_drop(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, column)) = data.downcast_ref::<ColumnRef>().map(|c| (c.app.clone(), c.column)) else {
        return Update::DoNothing;
    };
    let now = crate::state::now();
    crate::with_tasks(&mut app, &mut info, |_info, _app, s| {
        for id in s.take_dropped() {
            if let Some(i) = s.index_of(&id) {
                s.move_to_column(i, column, now);
            }
        }
    })
}
