//! The phone's quick-action sheet for a record (azul's `ActionSheet`): Open and what the record's page
//! offers (Edit, Check out, Check in, Log maintenance, Dispose, Export schedule), Delete last and in
//! red. It opens from a long press on a row of the table and lies over the whole frame; a scrim tap
//! or Escape closes it. A desktop has no sheet (it has the ribbon).
//!
//! The rows are the ribbon's own commands for the record the long press selected
//! ([`super::ribbon::record_commands`]): the sheet is another way to the same commands, not a second
//! list of them.

use azul::{
    callbacks::ActionSheetOnEventCallbackType,
    prelude::*,
    widgets::{ActionSheet, ActionSheetEvent, ActionSheetEventKind, ActionSheetItem},
};

use super::{preview, ribbon, run_action, with_erp, Erp};
use crate::{
    nav,
    views::spec::{ActionKind, ActionSpec},
};

/// Whether a command destroys: the Delete action.
#[must_use]
pub(crate) fn destroys(action: &ActionSpec) -> bool {
    matches!(&action.kind, ActionKind::Named(name) if name == "delete")
}

/// The sheet over the frame, when a phone has one open.
pub(crate) fn sheet(s: &Erp, app: &RefAny, phone: bool) -> Option<Dom> {
    if !phone || !s.sheet_open {
        return None;
    }
    let card = preview::card(&s.state)?;
    let commands = ribbon::record_commands(s);
    if commands.is_empty() {
        return None;
    }
    let danger: Vec<bool> = commands.iter().map(|(a, _)| destroys(a)).collect();
    let mut sheet = ActionSheet::create()
        .with_title(card.title.as_str(), card.subtitle.as_str())
        .with_open(true)
        .with_form(s.form)
        .with_on_event(app.clone(), on_sheet as ActionSheetOnEventCallbackType);
    for place in nav::sheet_order(&danger) {
        let (action, _) = &commands[place];
        let icon = if action.icon.is_empty() || action.icon == "ViewIcon" {
            "touch_app"
        } else {
            action.icon.as_str()
        };
        sheet = sheet.with_item(
            ActionSheetItem::create(action.label.as_str(), icon, place).with_danger(danger[place]),
        );
    }
    Some(sheet.dom())
}

/// A row chosen runs its command; a scrim tap closes the sheet.
extern "C" fn on_sheet(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: ActionSheetEvent,
) -> Update {
    with_erp(&mut data, &mut info, |s, _| {
        let chosen = match event.kind {
            ActionSheetEventKind::ItemChosen => {
                ribbon::record_commands(s).into_iter().nth(event.tag)
            }
            ActionSheetEventKind::Dismissed => None,
        };
        s.sheet_open = false;
        if let Some((action, params)) = chosen {
            run_action(s, &action.kind, &params);
        }
    })
}

#[cfg(test)]
#[path = "sheet_test.rs"]
mod sheet_test;
