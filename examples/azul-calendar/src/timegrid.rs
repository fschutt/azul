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
    root_dom, settings, views, views_ui, week, CalState, EventRef, CLIPPED_LINE, CLIPPED_TITLE,
    DAY_PAINT, DRAFT_PAINT, DRAFT_TITLE, ERROR, LINE, NOW_LINE, POPOVER, SECONDARY,
    SELECTED_RING, TODAY_PAINT, UNTITLED,
};

/// Width of the hour labels left of the days.
const GUTTER_PX: u32 = 56;
/// The `id` of the hours' scroll area.
pub(crate) const WEEK_SCROLL_ID: &str = "week-scroll";
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
        .with_id("all-day")
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
        .with_id("week-grid")
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
        .with_id(WEEK_SCROLL_ID)
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
        .with_id(format!("all-day-{index}"))
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
fn day_column(
    s: &CalState,
    index: usize,
    date: NaiveDate,
    now: Option<u32>,
    app: &RefAny,
) -> Dom {
    let hour = s.hour_px;
    let paint = if now.is_some() { TODAY_PAINT } else { DAY_PAINT };
    let target = RefAny::new(DayRef {
        app: app.clone(),
        day: index,
    });
    let mut column = Dom::create_div()
        .with_id(format!("day-{index}"))
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
        column.add_child(Dom::create_div().with_id("now-line").with_css(format!(
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
fn event_block(
    s: &CalState,
    e: &Event,
    date: NaiveDate,
    p: &week::Placement,
    app: &RefAny,
) -> Dom {
    let hour_px = s.hour_px;
    let top = week::y_of_minute(p.top as f32, hour_px);
    let height = week::y_of_minute(p.height as f32, hour_px).max(week::MIN_BLOCK_PX);
    let width = 100.0 / p.lanes.max(1) as f32;
    let left = width * p.lane as f32;
    let selected = views_ui::is_selected(s, &e.id, date);
    let time = week::time_range(e.start, e.end);
    let mut dom = Dom::create_div()
        .with_id(views_ui::occurrence_dom_id(&e.id, date))
        .with_css(format!(
            "position: absolute; top: {top:.3}px; left: {left:.3}%; width: {width:.3}%; \
             height: {height:.3}px; box-sizing: border-box; display: flex; flex-direction: \
             column; padding: 3px 6px; {} border-radius: 4px; font-size: 12px; overflow: \
             hidden; {}",
            s.colour_of(e).event_css(),
            if selected { SELECTED_RING } else { "" }
        ))
        .with_child(Dom::create_span_with_text(e.title.as_str()).with_css(CLIPPED_TITLE))
        .with_child(Dom::create_span_with_text(time.as_str()).with_css(CLIPPED_LINE));
    if !e.location.is_empty() {
        dom.add_child(Dom::create_span_with_text(e.location.as_str()).with_css(CLIPPED_LINE));
    }
    if let Some(m) = &e.meeting {
        if m.pending {
            dom.add_child(
                Dom::create_span_with_text("AzMeet link waits for the server").with_css(
                    format!("{CLIPPED_LINE} font-size: 11px; font-style: italic;"),
                ),
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
        .with_id("draft")
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
        .with_id("draft-panel")
        .with_css(POPOVER)
        .with_child(
            TextInput::create()
                .with_text(d.title.as_str())
                .with_placeholder("Add title")
                .with_on_text_input(app.clone(), on_draft_title)
                .with_on_virtual_key_down(app.clone(), on_draft_title_key)
                .dom()
                .with_id("draft-title"),
        )
        .with_child(
            Dom::create_span_with_text(week::draft_label(d.date, d.start, d.end))
                .with_id("draft-when")
                .with_css("font-size: 13px; color: system:secondary-text; margin-top: 10px;"),
        )
        .with_child(meet_toggle(d, app));
    if !d.error.is_empty() {
        panel.add_child(Dom::create_span_with_text(d.error.as_str()).with_css(ERROR));
    }
    panel.with_child(
        Dom::create_div()
            .with_css(
                "display: flex; flex-direction: row; align-items: center; margin-top: 16px;",
            )
            .with_child(
                Button::create("More options")
                    .with_on_click(app.clone(), on_draft_more)
                    .dom()
                    .with_id("draft-more")
                    .with_css("margin-right: auto;"),
            )
            .with_child(
                Button::create("Cancel")
                    .with_on_click(app.clone(), on_draft_cancel)
                    .dom()
                    .with_id("draft-cancel")
                    .with_css("margin-right: 8px;"),
            )
            .with_child(
                Button::with_type("Save", ButtonType::Primary)
                    .with_on_click(app.clone(), on_draft_save)
                    .dom()
                    .with_id("draft-save"),
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
                    .dom(),
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

