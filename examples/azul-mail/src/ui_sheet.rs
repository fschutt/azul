//! The phone's quick-action sheet for a message (azul's `ActionSheet`): Reply, Reply all, Forward,
//! Follow up, Unread / Read, Move, Archive and, last and in red, Delete. It opens from the list's
//! "open" of a row (the list reports no long press yet) and lies over the whole frame; a scrim
//! tap or Escape closes it. A desktop has no sheet (it has the ribbon and the menus).
//!
//! The rows run the ribbon's own commands (`ui_main::run_action`) on the message the sheet was
//! opened for, which a tap on its row had selected and opened.

use azul::{
    callbacks::ActionSheetOnEventCallbackType,
    prelude::*,
    widgets::{ActionSheet, ActionSheetEvent, ActionSheetEventKind, ActionSheetItem},
};
use azul_appkit::l10n::{label, t};

use crate::{
    nav::{self, SheetAction},
    ui_main::{self, Action},
    with_app, MailApp,
};

/// The ribbon command a sheet row runs.
#[must_use]
pub(crate) const fn command(action: SheetAction) -> Action {
    match action {
        SheetAction::Reply => Action::Reply,
        SheetAction::ReplyAll => Action::ReplyAll,
        SheetAction::Forward => Action::Forward,
        SheetAction::Archive => Action::Archive,
        SheetAction::Delete => Action::Delete,
        SheetAction::ToggleFlag => Action::ToggleFlag,
        SheetAction::ToggleRead => Action::ToggleRead,
        SheetAction::Move => Action::Move,
    }
}

/// Why a row cannot run now (a key of the resources), or `None` where it can: the answers need
/// the message open, Archive and Move need an Azlin account's folders.
#[must_use]
pub(crate) fn disabled_reason(action: SheetAction, message_open: bool, folders_changeable: bool) -> Option<&'static str> {
    match action {
        SheetAction::Reply | SheetAction::ReplyAll | SheetAction::Forward if !message_open => {
            Some("azmail-notice-select-first")
        }
        SheetAction::Archive | SheetAction::Move if !folders_changeable => Some(ui_main::READ_ONLY),
        _ => None,
    }
}

/// The sheet over the frame, when a phone has one open.
pub(crate) fn sheet(s: &MailApp, app: &RefAny, phone: bool) -> Option<Dom> {
    let uid = s.sheet.filter(|_| phone)?;
    let entry = s.entries.iter().find(|e| e.uid == uid)?;
    let subject = if entry.subject.is_empty() {
        t("azmail-no-subject")
    } else {
        entry.subject.clone()
    };
    let message_open = s.open.as_ref().is_some_and(|o| o.view.is_some() && o.entry.uid == uid);
    let changeable = ui_main::shows_azlin(s);
    let mut sheet = ActionSheet::create()
        .with_title(subject.as_str(), entry.from.as_str())
        .with_open(true)
        .with_form(s.form)
        .with_on_event(app.clone(), on_sheet as ActionSheetOnEventCallbackType);
    for (i, row) in nav::SHEET.iter().enumerate() {
        let mut item = ActionSheetItem::create(label(row.label), row.icon, nav::sheet_tag(i)).with_danger(row.danger);
        if let Some(why) = disabled_reason(row.action, message_open, changeable) {
            item = item.with_disabled_reason(t(why));
        }
        sheet = sheet.with_item(item);
    }
    Some(sheet.dom())
}

/// A row chosen runs its command; a scrim tap closes the sheet.
extern "C" fn on_sheet(mut data: RefAny, mut info: CallbackInfo, event: ActionSheetEvent) -> Update {
    let action = with_app(&mut data, |s, _| {
        s.sheet = None;
        match event.kind {
            ActionSheetEventKind::ItemChosen => nav::sheet_row_of(event.tag).map(|row| row.action),
            ActionSheetEventKind::Dismissed => None,
        }
    })
    .flatten();
    match action {
        Some(action) => ui_main::run_action(&mut data, &mut info, command(action)),
        None => Update::RefreshDom,
    }
}

#[cfg(test)]
#[path = "ui_sheet_test.rs"]
mod ui_sheet_test;
