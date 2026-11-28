//! The phone's quick-action sheets (azul's `ActionSheet`): an event's (a long press on an event:
//! Edit, Join meeting, Copy meeting link, Delete This Occurrence, Delete) and File's (the `...` of
//! the location: Info, Open & Export, Print, Calendars, Options, About). They lie over the whole
//! frame; a scrim tap or Escape closes them. A desktop has none (it has the ribbon, the menus and
//! the Preview's buttons).
//!
//! The event's rows run the same functions the Preview's buttons run ([`run_event_action`]); File's
//! rows open the page's dialog (`ui_dialogs`).

use azul::{
    callbacks::ActionSheetOnEventCallbackType,
    prelude::*,
    widgets::{ActionSheet, ActionSheetEvent, ActionSheetEventKind, ActionSheetItem},
};
use azul_appkit::l10n::{label, t, t_args, Arg};
use chrono::NaiveDate;

use crate::{
    chrome, editor_ui,
    nav::{self, EventAction},
    ui_preview, CalState,
};

/// What a sheet is open for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SheetFor {
    /// File's old items.
    File,
    /// An event (its id and the day of the occurrence the long press was on).
    Event { id: String, date: NaiveDate },
}

/// The sheet over the frame, when a phone has one open.
pub(crate) fn sheet(s: &CalState, app: &RefAny, phone: bool) -> Option<Dom> {
    let kind = s.sheet.as_ref().filter(|_| phone)?;
    let mut sheet = ActionSheet::create()
        .with_open(true)
        .with_form(s.form)
        .with_on_event(app.clone(), on_sheet as ActionSheetOnEventCallbackType);
    match kind {
        SheetFor::File => {
            sheet = sheet.with_title(
                t("azcalendar-file-sheet-title").as_str(),
                t("azcalendar-file-sheet-detail").as_str(),
            );
            for (i, (name, icon, _)) in nav::FILE_ITEMS.iter().enumerate() {
                sheet = sheet.with_item(ActionSheetItem::create(label(name), *icon, i));
            }
        }
        SheetFor::Event { id, date } => {
            let event = s.events.iter().find(|e| e.id == *id)?;
            let title = if event.title.trim().is_empty() {
                t(crate::UNTITLED)
            } else {
                event.title.clone()
            };
            sheet = sheet.with_title(title.as_str(), ui_preview::when_text(event, *date).as_str());
            for i in nav::event_sheet_rows(event.meeting.is_some(), event.repeat.is_some()) {
                let (name, icon, _, danger) = nav::EVENT_SHEET[i];
                sheet = sheet.with_item(ActionSheetItem::create(label(name), icon, i).with_danger(danger));
            }
        }
    }
    Some(sheet.dom())
}

/// A row chosen runs its command; a scrim tap closes the sheet.
extern "C" fn on_sheet(mut data: RefAny, mut info: CallbackInfo, event: ActionSheetEvent) -> Update {
    let kind = data
        .downcast_mut::<CalState>()
        .and_then(|mut s| s.sheet.take());
    let Some(kind) = kind else {
        return Update::RefreshDom;
    };
    if event.kind != ActionSheetEventKind::ItemChosen {
        return Update::RefreshDom;
    }
    match kind {
        SheetFor::File => {
            let Some((_, _, page)) = nav::FILE_ITEMS.get(event.tag) else {
                return Update::RefreshDom;
            };
            if let Some(mut s) = data.downcast_mut::<CalState>() {
                chrome::open_backstage(&mut s, *page);
            }
            Update::RefreshDom
        }
        SheetFor::Event { id, date } => match nav::EVENT_SHEET.get(event.tag) {
            Some((_, _, action, _)) => run_event_action(&mut data, &mut info, *action, &id, date),
            None => Update::RefreshDom,
        },
    }
}

/// Runs `action` on the event `id` (the occurrence of `date`): what a sheet's row and the Preview's
/// buttons do.
pub(crate) fn run_event_action(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    action: EventAction,
    id: &str,
    date: NaiveDate,
) -> Update {
    match action {
        EventAction::Edit => editor_ui::open_event(data, info, id, date),
        EventAction::JoinMeeting => crate::join_meeting(data, info, id),
        EventAction::CopyLink => {
            let Some(mut s) = data.downcast_mut::<CalState>() else {
                return Update::DoNothing;
            };
            let link = s
                .events
                .iter()
                .find(|e| e.id == id)
                .and_then(|e| e.meeting.as_ref())
                .map(|m| m.link.clone());
            let Some(link) = link else {
                return Update::DoNothing;
            };
            crate::copy_text(info, &link);
            s.notice = t("azcalendar-link-copied");
            Update::RefreshDom
        }
        EventAction::DeleteOccurrence => {
            let Some(mut s) = data.downcast_mut::<CalState>() else {
                return Update::DoNothing;
            };
            chrome::skip_occurrence(&mut s, id, date);
            Update::RefreshDom
        }
        EventAction::Delete => {
            let Some(mut s) = data.downcast_mut::<CalState>() else {
                return Update::DoNothing;
            };
            chrome::delete_event(&mut s, id);
            Update::RefreshDom
        }
    }
}

/// What the notice says after a delete: the title (never empty).
#[must_use]
pub(crate) fn deleted_text(title: &str) -> String {
    let title = if title.trim().is_empty() { t(crate::UNTITLED) } else { title.to_string() };
    t_args("azcalendar-deleted-event", &[("title", Arg::from(title))])
}

#[cfg(test)]
#[path = "ui_sheet_test.rs"]
mod ui_sheet_test;
