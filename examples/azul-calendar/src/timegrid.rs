//! The hours views: Day, Work Week and Week - CAL2's week, its days now the view's.
//!
//! The day-header row and the all-day row stay put while the hours scroll under them, with the
//! hour labels (`#week-scroll` holds `#week-grid`: the hour labels and the day columns,
//! `#day-0` ..). A pinch on the trackpad, or the wheel with Ctrl / Cmd held, zooms the hours
//! (20 to 240 px an hour) around the time under the pointer; a plain wheel scrolls. Events
//! that overlap sit side by side, in their calendar's colour; today's column has the red "now"
//! line.
//!
//! Clicking empty time makes a draft event there (an hour from the quarter hour clicked);
//! dragging over empty time makes one over the dragged quarter hours. The draft shows dashed,
//! with a popover (a `<transient-window>`): its title (focused), the day and times, "Add AzMeet
//! link", "More options" (the event editor window), Cancel and Save. Enter or Save saves it;
//! Escape, Cancel or a press outside drops it. A press on an event selects it, a double click
//! (or Enter on it) opens it in the editor.

use std::time::Instant;

use azul::{
    css::WindowBackgroundMaterial,
    dom::{DomNodeId, NodeHierarchyItemId, NodeId, VirtualKeyCode},
    misc::{TransientAnchor, TransientDismiss},
    prelude::*,
    time::SystemTimeDiff,
    widgets::{ButtonType, CheckBoxState, OnTextInputReturn, TextInputState, TextInputValid},
    window::TransientWindowConfig,
};
use chrono::{NaiveDate, NaiveTime};

use crate::{
    editor::EditorForm,
    editor_ui,
    event::{self, Event, Meeting},
    ids,
    root_dom, settings, views, views_ui, week, CalState, EventRef, CLIPPED_LINE, CLIPPED_TITLE,
    DAY_PAINT, DRAFT_PAINT, DRAFT_TITLE, ERROR, LINE, NOW_LINE, POPOVER, SECONDARY, SELECTED_RING,
    TODAY_PAINT, UNTITLED,
};

/// Width of the hour labels left of the days.
const GUTTER_PX: u32 = 56;
/// A press this soon after the popover closed by a click outside it is that click: it makes no
/// new draft (on some systems the popover's own window reports the click first).
const DISMISSING_PRESS: std::time::Duration = std::time::Duration::from_millis(250);
/// What the popover's "Add AzMeet link" line says once it is ticked.
const WILL_MINT: &str = "A new AzMeet link is made when you save. It works offline too: the \
                         meeting server gets it as soon as it answers.";
/// How long after a zoom step the zoom is saved: one write a second at most, not one per step.
const ZOOM_SAVE_DELAY_MS: u64 = 1000;

/// A press on empty time in a day's column: a click, or the start of a drag.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Press {
    /// The column (its index in the view's days).
    pub(crate) day: usize,
    /// Where it went down, and where the pointer is now: y in the column (from midnight), px.
    pub(crate) from_y: f32,
    pub(crate) to_y: f32,
    /// It moved far enough to be a drag.
    pub(crate) dragging: bool,
}

impl Press {
    /// The event this press makes if it is let go now, `(start, end)` in minutes.
    fn range(&self, hour_px: f32) -> (u32, u32) {
        if self.dragging || week::is_drag(self.from_y, self.to_y) {
            week::drag_range(
                week::minute_at_y(self.from_y, hour_px),
                week::minute_at_y(self.to_y, hour_px),
            )
        } else {
            week::click_range(week::minute_at_y(self.from_y, hour_px))
        }
    }
}

/// The draft a click or a drag made, and its popover's form.
pub(crate) struct Draft {
    pub(crate) serial: u32,
    /// The new event's id, fixed when the draft is made: its file's name.
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) date: NaiveDate,
    pub(crate) start: NaiveTime,
    pub(crate) end: NaiveTime,
    pub(crate) add_meet: bool,
    /// The link made for this draft, kept when its event could not be written, so the next
    /// Save uses the same one.
    pub(crate) link: Option<Meeting>,
    pub(crate) error: String,
}

/// The view's days: one for Day, Monday to Friday for Work Week, the week for Week.
pub(crate) fn days(s: &CalState) -> Vec<NaiveDate> {
    views::days_shown(s.view, s.anchor)
}

/// The timed events of the shown calendars on `day`, in the order `views::occurrences` gives.
fn timed_on<'a>(s: &'a CalState, day: NaiveDate) -> Vec<&'a Event> {
    s.occurrences(day, day)
        .into_iter()
        .map(|o| &s.events[o.index])
        .filter(|e| !e.all_day)
        .collect()
}

/// Where the timed events of the view's day `day` (an index) sit in its column.
fn placements_of(s: &CalState, day: usize) -> Vec<week::Placement> {
    days(s)
        .get(day)
        .map(|date| week::lay_out_day(&timed_on(s, *date)))
        .unwrap_or_default()
}

// ==== Layout ====

/// The hours view: the day-header row and the all-day row over the scroll area of the hours.
/// Every flex box down to the scroll area has `min-height: 0`, or the area would grow to the
/// day's height instead of scrolling over it.
pub(crate) fn time_grid(s: &CalState, app: &RefAny) -> Dom {
    let days = days(s);
    let gutter = format!("width: {GUTTER_PX}px; flex-shrink: 0;");
    let hour = s.hour_px;
    let mut header = Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; flex-shrink: 0; {DAY_PAINT} border-bottom: \
             1px solid {LINE};"
        ))
        .with_child(Dom::create_div().with_css(gutter.as_str()));
    let mut all_day = Dom::create_div()
        .with_id(ids::ALL_DAY)
        .with_css(format!(
            "display: flex; flex-direction: row; flex-shrink: 0; {DAY_PAINT} border-bottom: \
             1px solid {LINE};"
        ))
        .with_child(
            Dom::create_div()
                .with_css(format!(
                    "{gutter} padding: 2px 6px 2px 0px; font-size: 11px; {SECONDARY} \
                     text-align: right; box-sizing: border-box;"
                ))
                .with_child(Dom::create_span_with_text("All day")),
        );
    let mut hours = Dom::create_div().with_css(gutter.as_str());
    for h in 0..24 {
        hours.add_child(
            Dom::create_div()
                .with_css(format!(
                    "height: {hour:.3}px; padding-right: 6px; font-size: 11px; {SECONDARY} \
                     text-align: right; box-sizing: border-box;"
                ))
                .with_child(Dom::create_span_with_text(week::hour_label(h))),
        );
    }
    let mut grid = Dom::create_div()
        .with_id(ids::WEEK_GRID)
        .with_css(format!(
            "display: flex; flex-direction: row; flex-shrink: 0; height: {:.3}px;",
            week::day_height(hour)
        ))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Scroll),
            app.clone(),
            on_week_wheel,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::PinchIn),
            app.clone(),
            on_week_pinch,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::PinchOut),
            app.clone(),
            on_week_pinch,
        )
        .with_child(hours);
    let now = week::minute_of_day(chrono::Local::now().time());
    for (index, date) in days.iter().enumerate() {
        let today = *date == s.today;
        header.add_child(day_header(*date, today));
        all_day.add_child(all_day_cell(s, index, *date, app));
        grid.add_child(day_column(s, index, *date, today.then_some(now), app));
    }
    let scroll = Dom::create_div()
        .with_id(ids::WEEK_SCROLL)
        .with_css(
            "flex-grow: 1; flex-basis: 0px; min-height: 0; overflow-y: auto; overflow-x: hidden;",
        )
        .with_callback(
            EventFilter::Component(ComponentEventFilter::AfterMount),
            app.clone(),
            on_week_mounted,
        )
        .with_child(grid);
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; min-width: 0; min-height: 0; \
             padding: 0 8px 8px 0;",
        )
        .with_child(header)
        .with_child(all_day)
        .with_child(scroll)
}

fn day_header(date: NaiveDate, today: bool) -> Dom {
    let colour = if today {
        "color: system:accent; font-weight: bold;"
    } else {
        ""
    };
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: column; flex-grow: 1; flex-basis: 0px; min-width: \
             0; padding: 6px 8px; border-left: 1px solid {LINE}; {colour}"
        ))
        .with_child(Dom::create_span_with_text(week::day_label(date)))
}

/// A day's all-day events (`#all-day-<index>`): one bar each, in its calendar's colour.
fn all_day_cell(s: &CalState, index: usize, date: NaiveDate, app: &RefAny) -> Dom {
    let mut cell = Dom::create_div()
        .with_id(ids::all_day_column(index))
        .with_css(format!(
            "display: flex; flex-direction: column; flex-grow: 1; flex-basis: 0px; min-width: \
             0; min-height: 22px; padding: 2px; border-left: 1px solid {LINE}; box-sizing: \
             border-box;"
        ));
    for o in s.occurrences(date, date) {
        let e = &s.events[o.index];
        if !e.all_day {
            continue;
        }
        let selected = views_ui::is_selected(s, &e.id, o.first);
        cell.add_child(views_ui::interactive(
            Dom::create_div()
                .with_css(format!(
                    "{} border-radius: 3px; padding: 1px 6px; margin-bottom: 2px; font-size: \
                     12px; {}",
                    s.colour_of(e).bar_css(),
                    if selected { SELECTED_RING } else { "" }
                ))
                .with_child(Dom::create_span_with_text(e.title.as_str()).with_css(CLIPPED_TITLE)),
            app,
            &e.id,
            o.first,
            format!("{}, all day", e.title),
        ));
    }
    cell
}

/// A day's column (`#day-<index>`): the hour lines, the events, the "now" line on today, and
/// the draft with its popover. A press on empty time starts a click or a drag.
fn day_column(s: &CalState, index: usize, date: NaiveDate, now: Option<u32>, app: &RefAny) -> Dom {
    let hour = s.hour_px;
    let paint = if now.is_some() {
        TODAY_PAINT
    } else {
        DAY_PAINT
    };
    let target = RefAny::new(DayRef {
        app: app.clone(),
        day: index,
    });
    let mut column = Dom::create_div()
        .with_id(ids::day_column(index))
        .with_css(format!(
            "position: relative; flex-grow: 1; flex-basis: 0px; min-width: 0; height: \
             {:.3}px; border-left: 1px solid {LINE}; {paint}",
            week::day_height(hour)
        ))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::LeftMouseDown),
            target.clone(),
            on_day_press,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseMove),
            target.clone(),
            on_day_drag,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::LeftMouseUp),
            target,
            on_day_release,
        );
    for _ in 0..24 {
        column.add_child(Dom::create_div().with_css(format!(
            "height: {hour:.3}px; border-top: 1px solid {LINE}; box-sizing: border-box;"
        )));
    }
    let list = timed_on(s, date);
    for p in week::lay_out_day(&list) {
        column.add_child(event_block(s, list[p.index], date, &p, app));
    }
    if let Some(now) = now {
        column.add_child(Dom::create_div().with_id(ids::NOW_LINE).with_css(format!(
            "position: absolute; left: 0px; width: 100%; top: {:.3}px; height: 2px; {NOW_LINE}",
            week::y_of_minute(now as f32, hour)
        )));
    }
    if let Some(d) = draft_here(s, index) {
        column.add_child(draft_block(s, d, app));
    }
    column
}

/// One event in its day's column (`#event-<id>-<yyyymmdd>`): title, time, place and, with a
/// meeting link, "Join meeting" or that the link waits for the server.
fn event_block(s: &CalState, e: &Event, date: NaiveDate, p: &week::Placement, app: &RefAny) -> Dom {
    let hour_px = s.hour_px;
    let top = week::y_of_minute(p.top as f32, hour_px);
    let height = week::y_of_minute(p.height as f32, hour_px).max(week::MIN_BLOCK_PX);
    let width = 100.0 / p.lanes.max(1) as f32;
    let left = width * p.lane as f32;
    let selected = views_ui::is_selected(s, &e.id, date);
    let time = week::time_range(e.start, e.end);
    let mut dom = Dom::create_div()
        .with_id(ids::occurrence(&e.id, date))
        .with_css(format!(
            "position: absolute; top: {top:.3}px; left: {left:.3}%; width: {width:.3}%; \
             height: {height:.3}px; box-sizing: border-box; display: flex; flex-direction: \
             column; padding: 3px 6px; {} border-radius: 4px; font-size: 12px; overflow: \
             hidden; {}",
            s.colour_of(e).event_css(),
            if selected { SELECTED_RING } else { "" }
        ));
    // Only the lines that fit (LOOK: a 15-minute block squeezed two lines into 6 px each): a
    // short block says "title, time" on one line.
    let lines = week::block_lines(height);
    if lines == 1 {
        dom.add_child(
            Dom::create_span_with_text(format!("{}, {time}", e.title)).with_css(CLIPPED_TITLE),
        );
    } else {
        dom.add_child(Dom::create_span_with_text(e.title.as_str()).with_css(CLIPPED_TITLE));
        dom.add_child(Dom::create_span_with_text(time.as_str()).with_css(CLIPPED_LINE));
    }
    if lines >= 3 && !e.location.is_empty() {
        dom.add_child(Dom::create_span_with_text(e.location.as_str()).with_css(CLIPPED_LINE));
    }
    // The meeting's line stays whatever the height (its "Join meeting" is how one joins from
    // the week); the block clips what does not fit.
    if let Some(m) = &e.meeting {
        if m.pending {
            dom.add_child(
                Dom::create_span_with_text("AzMeet link waits for the server").with_css(format!(
                    "{CLIPPED_LINE} font-size: 11px; font-style: italic;"
                )),
            );
        } else {
            let target = RefAny::new(EventRef {
                app: app.clone(),
                id: e.id.clone(),
            });
            dom.add_child(
                Button::create("Join meeting")
                    .with_on_click(target, crate::on_join_meeting)
                    .dom()
                    .with_css("margin-top: 3px;"),
            );
        }
    }
    let name = format!("{}, {time}", e.title);
    views_ui::interactive(dom, app, &e.id, date, name)
}

/// The draft, when it is on the view's day `index`.
fn draft_here(s: &CalState, index: usize) -> Option<DraftShown> {
    if let Some(press) = s.press.filter(|p| p.dragging && p.day == index) {
        let (start, end) = press.range(s.hour_px);
        return Some(DraftShown {
            start,
            end,
            title: String::new(),
            popover: false,
        });
    }
    let d = s.draft.as_ref()?;
    let day = days(s).iter().position(|day| *day == d.date)?;
    (day == index).then(|| DraftShown {
        start: week::minute_of_day(d.start),
        end: week::minute_of_day(d.end),
        title: d.title.clone(),
        popover: true,
    })
}

/// The draft as the column shows it.
struct DraftShown {
    /// Minutes from midnight.
    start: u32,
    end: u32,
    title: String,
    /// The popover is open on it (not while a drag is still making it).
    popover: bool,
}

/// The draft (`#draft`): drawn like an event, dashed and pale, "(No title)" until it has one;
/// with its popover once the press that made it was let go.
fn draft_block(s: &CalState, draft: DraftShown, app: &RefAny) -> Dom {
    let hour = s.hour_px;
    let top = week::y_of_minute(draft.start as f32, hour);
    let minutes = draft.end.saturating_sub(draft.start) as f32;
    let height = week::y_of_minute(minutes, hour).max(week::MIN_BLOCK_PX);
    let title = if draft.title.trim().is_empty() {
        UNTITLED
    } else {
        draft.title.as_str()
    };
    let time = week::time_range(
        week::time_of_minute(draft.start),
        week::time_of_minute(draft.end),
    );
    let mut block = Dom::create_div()
        .with_id(ids::DRAFT)
        .with_css(format!(
            "position: absolute; top: {top:.3}px; left: 0px; width: 100%; height: \
             {height:.3}px; box-sizing: border-box; display: flex; flex-direction: column; \
             padding: 3px 6px; {DRAFT_PAINT} border-radius: 4px; font-size: 12px;"
        ))
        .with_child(
            // The text is clipped in a box of its own: the popover is the draft's child too.
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: column; flex-grow: 1; min-height: 0; \
                     overflow: hidden;",
                )
                .with_child(
                    Dom::create_span_with_text(title)
                        .with_css(format!("{CLIPPED_TITLE} {DRAFT_TITLE}")),
                )
                .with_child(Dom::create_span_with_text(time).with_css(CLIPPED_LINE)),
        );
    if draft.popover {
        if let Some(d) = &s.draft {
            block.add_child(popover(d, app));
        }
    }
    block
}

/// The draft's popover: a `<transient-window>` that opens to the right of the draft (the
/// engine flips it left at the screen's edge). A press outside it, Escape, or its window
/// losing focus dismisses it (`Dismissed`: the draft goes). It takes the keyboard, and the
/// engine focuses its first control, the title.
fn popover(d: &Draft, app: &RefAny) -> Dom {
    let config = TransientWindowConfig::opened()
        .with_anchor(TransientAnchor::Right)
        .with_dismiss(TransientDismiss::Outside)
        .with_material(WindowBackgroundMaterial::Transparent);
    let target = RefAny::new(DraftRef {
        app: app.clone(),
        serial: d.serial,
    });
    Dom::create_from_data(NodeData::create_node(NodeType::TransientWindow(config)))
        .with_callback(
            EventFilter::Component(ComponentEventFilter::Dismissed),
            target,
            on_popover_dismissed,
        )
        .with_child(popover_panel(d, app))
}

/// The popover's card: title (`#draft-title`), day and times (`#draft-when`), "Add AzMeet
/// link", and More options (`#draft-more`), Cancel (`#draft-cancel`) / Save (`#draft-save`).
fn popover_panel(d: &Draft, app: &RefAny) -> Dom {
    let mut panel = Dom::create_div()
        .with_id(ids::DRAFT_PANEL)
        .with_css(POPOVER)
        .with_child(
            TextInput::create()
                .with_text(d.title.as_str())
                .with_placeholder("Add title")
                .with_on_text_input(app.clone(), on_draft_title)
                .with_on_virtual_key_down(app.clone(), on_draft_title_key)
                .dom()
                .with_id(ids::DRAFT_TITLE),
        )
        .with_child(
            Dom::create_span_with_text(week::draft_label(d.date, d.start, d.end))
                .with_id(ids::DRAFT_WHEN)
                .with_css("font-size: 13px; color: system:secondary-text; margin-top: 10px;"),
        )
        .with_child(meet_toggle(d, app));
    if !d.error.is_empty() {
        panel.add_child(Dom::create_span_with_text(d.error.as_str()).with_css(ERROR));
    }
    panel.with_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center; margin-top: 16px;")
            .with_child(
                Button::create("More options")
                    .with_on_click(app.clone(), on_draft_more)
                    .dom()
                    .with_id(ids::DRAFT_MORE)
                    .with_css("margin-right: auto;"),
            )
            .with_child(
                Button::create("Cancel")
                    .with_on_click(app.clone(), on_draft_cancel)
                    .dom()
                    .with_id(ids::DRAFT_CANCEL)
                    .with_css("margin-right: 8px;"),
            )
            .with_child(
                Button::with_type("Save", ButtonType::Primary)
                    .with_on_click(app.clone(), on_draft_save)
                    .dom()
                    .with_id(ids::DRAFT_SAVE),
            ),
    )
}

/// "Add AzMeet link" (the box, and its label, which toggles it too) and what ticking it means.
fn meet_toggle(d: &Draft, app: &RefAny) -> Dom {
    let mut part = Dom::create_div().with_css("display: flex; flex-direction: column;");
    part.add_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center; margin-top: 16px;")
            .with_child(
                CheckBox::create(d.add_meet)
                    .with_on_toggle(app.clone(), on_draft_meet_toggled)
                    .with_accessibility_name("Add AzMeet link")
                    .dom()
                    .with_id(ids::DRAFT_MEET),
            )
            .with_child(
                Dom::create_span_with_text("Add AzMeet link")
                    .with_css("margin-left: 8px; cursor: pointer;")
                    .with_callback(
                        EventFilter::Hover(HoverEventFilter::Click),
                        app.clone(),
                        on_draft_meet_label,
                    ),
            ),
    );
    if d.add_meet {
        part.add_child(
            Dom::create_span_with_text(WILL_MINT)
                .with_css("font-size: 12px; color: system:secondary-text; margin-top: 4px;"),
        );
    }
    part
}

// ==== Scroll, zoom ====

/// The hours' scroll area as a callback sees it: its node, where it is in the window, and how
/// far it is scrolled.
struct WeekScroll {
    node: NodeHierarchyItemId,
    top: f32,
    height: f32,
    scroll_y: f32,
}

fn week_scroll(info: &CallbackInfo) -> Option<WeekScroll> {
    let node = info.get_node_id_by_id_attribute(root_dom(), ids::WEEK_SCROLL);
    // 0 is "no node"; a node's raw id is its index + 1.
    let index = node.into_raw().checked_sub(1)?;
    let rect = info
        .get_node_rect(DomNodeId {
            dom: root_dom(),
            node,
        })
        .into_option()?;
    let scroll_y = info
        .get_scroll_offset_for_node(root_dom(), NodeId::create(index))
        .into_option()
        .map_or(0.0, |offset| offset.y);
    Some(WeekScroll {
        node,
        top: rect.origin.y,
        height: rect.size.height,
        scroll_y,
    })
}

/// Zooms the hours by `factor`, keeping the time under the pointer (`pointer_y`, a window y;
/// the view's middle without one) where it is; the zoom is saved a moment later.
fn zoom(
    s: &mut CalState,
    info: &mut CallbackInfo,
    app: &RefAny,
    factor: f32,
    pointer_y: Option<f32>,
) -> Update {
    let old = s.hour_px;
    let new = week::clamp_hour_px(old * factor);
    if (new - old).abs() < 0.01 {
        return Update::DoNothing;
    }
    s.hour_px = new;
    queue_zoom_save(s, info, app);
    if let Some(view) = week_scroll(info) {
        let pointer = pointer_y.map_or(view.height / 2.0, |y| {
            (y - view.top).clamp(0.0, view.height.max(0.0))
        });
        let y = week::zoom_scroll(old, new, pointer, view.scroll_y, view.height);
        // Unclamped: the day is taller after zooming in than the layout the offset is checked
        // against now; the rebuild this returns lays the new height out before anything draws.
        info.scroll_to_unclamped(root_dom(), view.node, LogicalPosition { x: 0.0, y });
    }
    Update::RefreshDom
}

/// Saves the zoom in the settings file a moment from now, unless that is queued already.
fn queue_zoom_save(s: &mut CalState, info: &mut CallbackInfo, app: &RefAny) {
    if s.zoom_save_queued {
        return;
    }
    s.zoom_save_queued = true;
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(app.clone(), on_save_zoom, get_time).with_delay(Duration::System(
            SystemTimeDiff::from_millis(ZOOM_SAVE_DELAY_MS),
        )),
    );
}

/// Writes the zoom as it is now into the settings file, for the next start.
extern "C" fn on_save_zoom(mut data: RefAny, _info: TimerCallbackInfo) -> TimerCallbackReturn {
    if let Some(mut s) = data.downcast_mut::<CalState>() {
        s.zoom_save_queued = false;
        let line = settings::hour_px_line(s.hour_px);
        s.save_setting(&line);
    }
    TimerCallbackReturn::terminate_unchanged()
}

/// The wheel over the hours: with Ctrl or Cmd held it zooms (and the hours do not scroll as
/// well); without, they scroll as any scroll area does.
extern "C" fn on_week_wheel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let modifiers = info.get_key_modifiers();
    if !modifiers.primary_down() {
        return Update::DoNothing;
    }
    let hit = info.get_hit_node();
    let node = NodeId::create(hit.node.into_raw().saturating_sub(1));
    let dy = info
        .get_scroll_delta(hit.dom, node)
        .into_option()
        .map_or(0.0, |delta| delta.y);
    if dy == 0.0 {
        return Update::DoNothing;
    }
    // The wheel has one consumer: this zoom. The scroll it would have made is taken back.
    info.prevent_default();
    let pointer_y = info
        .get_cursor_relative_to_viewport()
        .into_option()
        .map(|p| p.y);
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    zoom(
        &mut guard,
        &mut info,
        &app,
        week::wheel_zoom_factor(dy),
        pointer_y,
    )
}

/// A pinch over the hours zooms them around the pinch's centre.
extern "C" fn on_week_pinch(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(pinch) = info.get_pinch().into_option() else {
        return Update::DoNothing;
    };
    // Cumulative since the gesture began: the zoom is the ratio to the previous update.
    let sample = week::PinchSample {
        scale: pinch.scale,
        began: pinch.began,
    };
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let factor = week::pinch_step(s.last_pinch_scale, sample);
    s.last_pinch_scale = Some(sample.scale);
    zoom(s, &mut info, &app, factor, Some(pinch.center.y))
}

/// The hours open at 08:00, or an hour before now when the view shows today.
extern "C" fn on_week_mounted(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((minute, hour_px)) = data.downcast_ref::<CalState>().map(|s| {
        let today_shown = days(&s).contains(&s.today);
        (
            week::first_minute_shown(today_shown, chrono::Local::now().time()),
            s.hour_px,
        )
    }) else {
        return Update::DoNothing;
    };
    let scroll = info.get_hit_node();
    info.scroll_to(
        scroll.dom,
        scroll.node,
        LogicalPosition {
            x: 0.0,
            y: week::y_of_minute(minute as f32, hour_px),
        },
    );
    Update::DoNothing
}

/// Scrolls the view to an event on `date` at `start` that was just saved: another range of
/// days when the view does not show `date`, else the hours when it is out of sight.
pub(crate) fn reveal(s: &mut CalState, info: &mut CallbackInfo, date: NaiveDate, start: NaiveTime) {
    let (first, last) = views::visible_range(s.view, s.anchor);
    if date < first || date > last {
        s.set_anchor(date);
        return;
    }
    if !s.view.is_time_grid() {
        return;
    }
    let Some(view) = week_scroll(info) else {
        return;
    };
    let minute = week::minute_of_day(start);
    if let Some(y) = week::reveal_scroll(minute, s.hour_px, view.scroll_y, view.height) {
        info.scroll_to(root_dom(), view.node, LogicalPosition { x: 0.0, y });
    }
}

// ==== Click or drag to make an event ====

/// A day's column, for its callbacks.
struct DayRef {
    app: RefAny,
    /// The column: an index into the view's days.
    day: usize,
}

/// A draft, for a callback that must not act on a newer one.
struct DraftRef {
    app: RefAny,
    serial: u32,
}

/// A press in a day's column. On empty time it starts a click or a drag; on an event it is the
/// event's. While the popover is open, it is the press that closes it: the draft goes and
/// nothing new starts (Google Calendar's way).
extern "C" fn on_day_press(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, day)) = data
        .downcast_ref::<DayRef>()
        .map(|r| (r.app.clone(), r.day))
    else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    s.press = None;
    if s.draft.is_some() {
        s.draft = None;
        s.popover_closed_at = Some(Instant::now());
        return Update::RefreshDom;
    }
    if s.popover_closed_at
        .is_some_and(|closed| closed.elapsed() < DISMISSING_PRESS)
    {
        return Update::DoNothing;
    }
    let Some(at) = info.get_cursor_relative_to_node().into_option() else {
        return Update::DoNothing;
    };
    let width = info
        .get_hit_node_rect()
        .into_option()
        .map_or(0.0, |rect| rect.size.width);
    let x_frac = if width > 0.0 { at.x / width } else { 0.5 };
    if week::event_at(&placements_of(s, day), at.y, x_frac, s.hour_px).is_some() {
        return Update::DoNothing;
    }
    s.press = Some(Press {
        day,
        from_y: at.y,
        to_y: at.y,
        dragging: false,
    });
    // The drag goes on when the pointer leaves the column (or the window).
    let column = info.get_hit_node();
    info.capture_pointer(column);
    Update::DoNothing
}

/// The pointer moving over a day's column: a press on empty time that moved a few pixels is a
/// drag, and the draft follows it (redrawn when its quarter hours change).
extern "C" fn on_day_drag(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((mut app, day)) = data
        .downcast_ref::<DayRef>()
        .map(|r| (r.app.clone(), r.day))
    else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let hour_px = s.hour_px;
    let Some(press) = s.press.as_mut().filter(|p| p.day == day) else {
        return Update::DoNothing;
    };
    let Some(at) = info.get_cursor_relative_to_node().into_option() else {
        return Update::DoNothing;
    };
    let before = press.dragging.then(|| press.range(hour_px));
    press.to_y = at.y;
    if !press.dragging && !week::is_drag(press.from_y, press.to_y) {
        return Update::DoNothing;
    }
    press.dragging = true;
    if before == Some(press.range(hour_px)) {
        Update::DoNothing
    } else {
        Update::RefreshDom
    }
}

/// The press let go: a draft of what it made (an hour from a click, the quarter hours of a
/// drag), with its popover. The selection goes.
extern "C" fn on_day_release(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((mut app, day)) = data
        .downcast_ref::<DayRef>()
        .map(|r| (r.app.clone(), r.day))
    else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(mut press) = s.press.filter(|p| p.day == day) else {
        return Update::DoNothing;
    };
    s.press = None;
    if let Some(at) = info.get_cursor_relative_to_node().into_option() {
        press.to_y = at.y;
    }
    let (start, end) = press.range(s.hour_px);
    let Some(date) = days(s).get(day).copied() else {
        return Update::RefreshDom;
    };
    s.drafts_made += 1;
    s.draft = Some(Draft {
        serial: s.drafts_made,
        id: event::new_event_id(),
        title: String::new(),
        date,
        start: week::time_of_minute(start),
        end: week::time_of_minute(end),
        add_meet: false,
        link: None,
        error: String::new(),
    });
    s.selected = None;
    s.notice.clear();
    Update::RefreshDom
}

/// The popover was dismissed (a press outside it, Escape, its window losing focus): the draft
/// goes.
extern "C" fn on_popover_dismissed(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, serial)) = data
        .downcast_ref::<DraftRef>()
        .map(|r| (r.app.clone(), r.serial))
    else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    if !s.draft.as_ref().is_some_and(|d| d.serial == serial) {
        return Update::DoNothing;
    }
    s.draft = None;
    s.popover_closed_at = Some(Instant::now());
    Update::RefreshDom
}

// ==== The popover's form ====

extern "C" fn on_draft_title(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<CalState>() {
        if let Some(d) = s.draft.as_mut() {
            d.title = state.get_text().as_str().to_string();
        }
    }
    crate::typed()
}

/// Enter in the popover's title saves the event, as Save does.
extern "C" fn on_draft_title_key(
    mut data: RefAny,
    mut info: CallbackInfo,
    _state: TextInputState,
) -> OnTextInputReturn {
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    let update = if matches!(
        key,
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter)
    ) {
        save_draft(&mut data, &mut info)
    } else {
        Update::DoNothing
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// Ticks or clears "Add AzMeet link": `checked` from the box itself, or a toggle (its label).
fn set_draft_meet(data: &mut RefAny, checked: Option<bool>) -> Update {
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let Some(d) = s.draft.as_mut() else {
        return Update::DoNothing;
    };
    d.add_meet = checked.unwrap_or(!d.add_meet);
    d.error.clear();
    Update::RefreshDom
}

extern "C" fn on_draft_meet_toggled(
    mut data: RefAny,
    _info: CallbackInfo,
    state: CheckBoxState,
) -> Update {
    set_draft_meet(&mut data, Some(state.checked))
}

extern "C" fn on_draft_meet_label(mut data: RefAny, _info: CallbackInfo) -> Update {
    set_draft_meet(&mut data, None)
}

extern "C" fn on_draft_cancel(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    s.draft = None;
    Update::RefreshDom
}

extern "C" fn on_draft_save(mut data: RefAny, mut info: CallbackInfo) -> Update {
    save_draft(&mut data, &mut info)
}

/// "More options": the draft goes to the event editor window, with what it has so far.
extern "C" fn on_draft_more(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let form = {
        let Some(mut guard) = data.downcast_mut::<CalState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let calendar = s.calendar_for_new();
        let Some(d) = s.draft.take() else {
            return Update::DoNothing;
        };
        s.editors_opened += 1;
        let mut form =
            EditorForm::new_event(s.editors_opened, &d.id, d.date, d.start, d.end, &calendar);
        form.title = d.title;
        form.add_meet = d.add_meet;
        form.meeting = d.link;
        form
    };
    editor_ui::open_form(&mut data, &mut info, form, None)
}

/// Save (the button, or Enter in the popover's title): writes the event at once, in the first
/// calendar shown, "(No title)" without a title. With "Add AzMeet link" the link is made here,
/// pending, and its room is registered with the meeting server right after (or as soon as the
/// server answers), so saving never waits on the network.
fn save_draft(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let server = s.server.clone();
    let calendar = s.calendar_for_new();
    let Some(d) = s.draft.as_mut() else {
        return Update::DoNothing;
    };
    d.error.clear();
    let title = if d.title.trim().is_empty() {
        UNTITLED.to_string()
    } else {
        d.title.trim().to_string()
    };
    let meeting = if d.add_meet {
        Some(
            d.link
                .get_or_insert_with(|| crate::new_meeting(&server))
                .clone(),
        )
    } else {
        None
    };
    let made =
        Event::create(&d.id, &title, d.date, d.start, d.end, meeting.clone()).and_then(|mut e| {
            e.calendar = calendar;
            e.check()
        });
    let event = match made {
        Ok(event) => event,
        Err(e) => {
            d.error = crate::editor::error_text(&e);
            eprintln!("[azcalendar] cannot save: {}", d.error);
            return Update::RefreshDom;
        }
    };
    let (date, start) = (event.date, event.start);
    match s.store_event(event) {
        Ok(_) => {
            s.notice = match &meeting {
                Some(m) => format!("Saved \"{title}\" with the AzMeet link {}", m.link),
                None => format!("Saved \"{title}\"."),
            };
            s.draft = None;
            reveal(s, info, date, start);
            crate::sync_links(s, info, &app);
        }
        Err(message) => {
            eprintln!("[azcalendar] {message}");
            if let Some(d) = s.draft.as_mut() {
                d.error = message;
            }
        }
    }
    Update::RefreshDom
}
