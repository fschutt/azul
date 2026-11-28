//! The selected event in azul's `Preview`: the event ITSELF - its title, when it is, where, in which
//! calendar, how it repeats, and "Join meeting" when it has a link - and nothing else. What the
//! old window piled around it is behind small (?) buttons in the Preview's header: the attendees,
//! the notes, the reminder and the meeting link. What needs saying at once is the banner at the
//! very top: a meeting link that waits for the meeting server (its (?) says why and syncs now).
//!
//! On a desktop the Preview is docked at the right of the view; on a phone it is the bottom pane
//! (four detents, a tap on its grip toggles the peek and the half) and is there while an event is
//! selected. Edit and Delete are the two icon buttons at the end of its header.
//!
//! The sizes of the text written here are in `rem` (0.875 rem is 14 px at the default root size of
//! 16 px), never in px: they follow the root font size, so a system text size that the engine
//! applies at the root reaches them without a line of this file changing.

use azul::{
    callbacks::{ButtonOnClickCallbackType, PreviewOnEventCallbackType},
    prelude::*,
    shells::ShellEmptyState,
    str::String as AzString,
    widgets::{
        Button, ButtonType, Preview, PreviewBanner, PreviewBannerTone, PreviewDetail, PreviewEvent,
        PreviewEventKind,
    },
};
use azul_appkit::l10n::{label, t, t_args, t_said, Arg, DateStyle};
use chrono::Duration;

use crate::{
    calendars::Colour,
    editor::{reminder_index, REMINDERS},
    event::Event,
    ids, ui_sheet, week, CalState,
};

/// The widest or tallest a Preview is taken to be from what a drag reported, px.
pub(crate) const PREVIEW_SIZE_MAX: f32 = 4_000.0;

/// The most attendees the popover lists; an event with more says how many it leaves out.
pub(crate) const MAX_ATTENDEE_ROWS: usize = 50;

/// A pane size a drag reported, held to what the app keeps: a number that is no size is 0 (the
/// widget's default), a size beyond [`PREVIEW_SIZE_MAX`] is the limit.
#[must_use]
pub(crate) fn clamped_preview_size(size: f32) -> f32 {
    if size.is_finite() && size > 0.0 {
        size.min(PREVIEW_SIZE_MAX)
    } else {
        0.0
    }
}

/// Whether the pane is open: a desktop's dock always (it is empty and says so), a phone's bottom
/// pane while an event is selected.
#[must_use]
pub(crate) const fn is_open(phone: bool, selected: bool) -> bool {
    !phone || selected
}

/// The event the window selected and the day of the occurrence, when it is still there.
#[must_use]
pub(crate) fn selected_event(s: &CalState) -> Option<(&Event, chrono::NaiveDate)> {
    let (id, day) = s.selected.as_ref()?;
    s.events.iter().find(|e| e.id == *id).map(|e| (e, *day))
}

/// The Preview of the window now; `None` where the View tab switched it off (the body is the
/// whole window then).
pub(crate) fn preview(s: &CalState, app: &RefAny, phone: bool) -> Option<Preview> {
    if !s.show_preview {
        return None;
    }
    let selected = selected_event(s);
    let mut preview = Preview::create(Dom::create_div(), pane(s, app, selected))
        .with_title(title(selected.map(|(e, _)| e)).as_str())
        .with_open(is_open(phone, selected.is_some()))
        .with_size(clamped_preview_size(s.preview_size))
        .with_form(s.form)
        .with_dom_id(ids::PREVIEW)
        .with_on_event(app.clone(), on_preview as PreviewOnEventCallbackType);
    let Some((event, day)) = selected else {
        return Some(preview);
    };
    preview = preview.with_header_end(header_buttons(app));
    if !event.attendees.is_empty() {
        preview = preview.with_detail(
            PreviewDetail::create(label("azcalendar-detail-attendees"), attendees_dom(event)).with_icon("group"),
        );
    }
    if !event.notes.trim().is_empty() {
        preview = preview.with_detail(
            PreviewDetail::create(label("azcalendar-detail-notes"), notes_dom(event)).with_icon("notes"),
        );
    }
    if event.reminder.is_some() {
        preview = preview.with_detail(
            PreviewDetail::create(label("azcalendar-detail-reminder"), reminder_dom(event, day)).with_icon("alarm"),
        );
    }
    if let Some(meeting) = &event.meeting {
        preview = preview.with_detail(
            PreviewDetail::create(label("azcalendar-detail-link"), link_dom(app, &meeting.link, meeting.pending))
                .with_icon("link"),
        );
    }
    if let Some(banner) = banner(s, app, event) {
        preview = preview.with_banner(banner);
    }
    Some(preview)
}

/// The Preview's title: the selected event's, else "Event".
fn title(event: Option<&Event>) -> String {
    match event {
        Some(e) if !e.title.trim().is_empty() => e.title.clone(),
        Some(_) => t(crate::UNTITLED),
        None => t("azcalendar-preview-title"),
    }
}

/// What "when" says: "Sat 28 Nov, 10:00 - 11:00", "Sat 28 Nov, all day", "Fri 27 - Sun 29 Nov, all
/// day".
#[must_use]
pub(crate) fn when_text(event: &Event, day: chrono::NaiveDate) -> String {
    let said = |d| crate::day_text(DateStyle::ShortDate, d);
    if event.all_day {
        let last = day + Duration::days(event.span_days());
        if last > day {
            t_args(
                "azcalendar-when-all-day-range",
                &[("first", Arg::from(said(day))), ("last", Arg::from(said(last)))],
            )
        } else {
            t_args("azcalendar-when-all-day", &[("day", Arg::from(said(day)))])
        }
    } else {
        t_args(
            "azcalendar-when-time",
            &[
                ("day", Arg::from(said(day))),
                ("time", Arg::from(week::time_range(event.start, event.end))),
            ],
        )
    }
}

/// A line of the pane: an icon, then the text.
fn fact(icon: &str, text: &str) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; gap: 8px; min-width: 0px;")
        .with_child(Dom::create_icon(icon).with_css(
            "font-size: 1.125rem; width: 1.25em; flex-shrink: 0; text-align: center; \
             color: system:secondary-text;",
        ))
        .with_child(
            Dom::create_span_with_text(text)
                .with_css("font-size: 0.875rem; flex-grow: 1; min-width: 0px; overflow-wrap: anywhere;"),
        )
}

/// The calendar's line: a box in its colour, its name.
fn calendar_line(s: &CalState, event: &Event) -> Dom {
    let (name, colour) = s
        .calendar_of(event)
        .map_or_else(|| (String::new(), Colour::Blue), |c| (crate::calendar_name(c), c.colour));
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; gap: 8px; min-width: 0px;")
        .with_child(Dom::create_div().with_css(format!(
            "width: 0.75rem; height: 0.75rem; margin: 0px 0.25rem; border-radius: 3px; flex-shrink: 0; {}",
            colour.swatch_css()
        )))
        .with_child(
            Dom::create_span_with_text(name.as_str())
                .with_css("font-size: 0.875rem; min-width: 0px; overflow-wrap: anywhere;"),
        )
}

/// The pane: the event itself. With none selected, the empty state.
fn pane(s: &CalState, app: &RefAny, selected: Option<(&Event, chrono::NaiveDate)>) -> Dom {
    let Some((event, day)) = selected else {
        return ShellEmptyState::create(label("azcalendar-select-event"))
            .with_icon("event")
            .with_detail(label("azcalendar-select-event-detail"))
            .dom();
    };
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; gap: 6px; padding: 8px 14px; min-width: 0px;")
        .with_child(fact("schedule", &when_text(event, day)));
    if !event.location.trim().is_empty() {
        column.add_child(fact("place", event.location.trim()));
    }
    column.add_child(calendar_line(s, event));
    if let Some(rule) = &event.repeat {
        column.add_child(fact("repeat", &t_said(&rule.description(event.date))));
    }
    if event.meeting.is_some() {
        column.add_child(
            Button::with_type(label("azcalendar-sheet-join"), ButtonType::Primary)
                .with_on_click(app.clone(), on_join as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::PREVIEW_JOIN)
                .with_css("align-self: flex-start; margin-top: 4px;"),
        );
    }
    column
}

// ---- behind the (?) buttons ----

/// A row of a popover: a name and what it says.
fn row(text: &str) -> Dom {
    Dom::create_span_with_text(text).with_css("font-size: 0.875rem; overflow-wrap: anywhere;")
}

/// Behind the attendees' (?): their addresses, one to a line.
fn attendees_dom(event: &Event) -> Dom {
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; gap: 4px; max-width: 320px;");
    for who in event.attendees.iter().take(MAX_ATTENDEE_ROWS) {
        column.add_child(row(who));
    }
    if event.attendees.len() > MAX_ATTENDEE_ROWS {
        column.add_child(row(&t_args(
            "azcalendar-more-attendees",
            &[("count", Arg::from(event.attendees.len() - MAX_ATTENDEE_ROWS))],
        )));
    }
    column
}

/// Behind the notes' (?): the notes as they were typed.
fn notes_dom(event: &Event) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; max-width: 320px;")
        .with_child(row(event.notes.trim()))
}

/// What the reminder says: "15 minutes before" (a reminder that is none of the list's is said as
/// the nearest one before it).
#[must_use]
pub(crate) fn reminder_text(minutes: Option<u32>) -> String {
    t(REMINDERS[reminder_index(minutes)].1)
}

/// Behind the reminder's (?): when it rings.
fn reminder_dom(event: &Event, _day: chrono::NaiveDate) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; max-width: 320px;")
        .with_child(row(&reminder_text(event.reminder)))
}

/// Behind the meeting link's (?): the link, and a button that copies it; a link that is not
/// registered yet says so.
fn link_dom(app: &RefAny, link: &str, pending: bool) -> Dom {
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; gap: 8px; max-width: 320px;")
        .with_child(row(link));
    if pending {
        column.add_child(row(&t("azcalendar-preview-link-waiting")));
    }
    column.with_child(
        Button::with_type(label("azcalendar-copy-link"), ButtonType::Secondary)
            .with_on_click(app.clone(), on_copy_link as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::PREVIEW_COPY)
            .with_css("align-self: flex-start;"),
    )
}

/// The banner: a meeting link that waits for its server, or that the server refused.
fn banner(s: &CalState, app: &RefAny, event: &Event) -> Option<PreviewBanner> {
    event.meeting.as_ref().filter(|m| m.pending)?;
    let refused = s.sync_refused.get(&event.id);
    let (text, tone) = match refused {
        Some(_) => (t("azcalendar-preview-link-refused"), PreviewBannerTone::Warning),
        None => (t("azcalendar-preview-link-waiting"), PreviewBannerTone::Info),
    };
    let mut details = Dom::create_div()
        .with_css("display: flex; flex-direction: column; gap: 8px; max-width: 320px;");
    if let Some(why) = refused {
        details.add_child(row(&azul_appkit::l10n::t_text(why)));
    }
    details.add_child(
        Button::with_type(label("azcalendar-sync-meeting-links-now"), ButtonType::Primary)
            .with_on_click(app.clone(), crate::on_sync_now as ButtonOnClickCallbackType)
            .dom()
            .with_css("align-self: flex-start;"),
    );
    Some(PreviewBanner::create(text.as_str(), tone).with_icon("sync").with_details(details))
}

// ---- the header's buttons: Edit and Delete ----

/// Edit and Delete: two icon buttons (an icon OR text, never both) at the end of the header.
fn header_buttons(app: &RefAny) -> Dom {
    let icon_button = |icon: &str, name: &str, id: AzString, cb: ButtonOnClickCallbackType| {
        Button::create("")
            .with_icon(icon)
            .with_on_click(app.clone(), cb)
            .dom()
            .with_id(id)
            .with_accessibility_name(label(name))
    };
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center;")
        .with_child(icon_button("edit", "azcalendar-sheet-edit", ids::PREVIEW_EDIT, on_edit))
        .with_child(icon_button("delete", "azcalendar-delete", ids::PREVIEW_DELETE, on_delete))
}

/// Runs `action` on the selected event.
fn run(data: &mut RefAny, info: &mut CallbackInfo, action: crate::nav::EventAction) -> Update {
    let Some((id, day)) = data
        .downcast_ref::<CalState>()
        .and_then(|s| s.selected.clone())
    else {
        return Update::DoNothing;
    };
    ui_sheet::run_event_action(data, info, action, &id, day)
}

extern "C" fn on_edit(mut data: RefAny, mut info: CallbackInfo) -> Update {
    run(&mut data, &mut info, crate::nav::EventAction::Edit)
}

extern "C" fn on_delete(mut data: RefAny, mut info: CallbackInfo) -> Update {
    run(&mut data, &mut info, crate::nav::EventAction::Delete)
}

extern "C" fn on_join(mut data: RefAny, mut info: CallbackInfo) -> Update {
    run(&mut data, &mut info, crate::nav::EventAction::JoinMeeting)
}

extern "C" fn on_copy_link(mut data: RefAny, mut info: CallbackInfo) -> Update {
    run(&mut data, &mut info, crate::nav::EventAction::CopyLink)
}

/// The pane's size as a drag reports it: kept for the next build.
extern "C" fn on_preview(mut data: RefAny, _info: CallbackInfo, event: PreviewEvent) -> Update {
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    s.preview_size = clamped_preview_size(event.size);
    // while the pane is dragged the widget moves it; a rebuild is for where it settled
    if event.kind == PreviewEventKind::Settled {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

#[cfg(test)]
#[path = "ui_preview_test.rs"]
mod ui_preview_test;
