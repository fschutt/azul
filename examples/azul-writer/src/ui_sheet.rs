//! The phone's quick-action sheet for an open document (azul's `ActionSheet`): Show, switch
//! collaboration on or off, Close. It opens from a long press on a document's row of the sidebar
//! and lies over the whole frame; a scrim tap or Escape closes it. A desktop has no sheet (it has
//! the ribbon, the File pages and the keys).

use azul::{
    callbacks::{ActionSheetOnEventCallbackType, CallbackInfo, RefAny, Update},
    dom::Dom,
    widgets::{ActionSheet, ActionSheetEvent, ActionSheetEventKind, ActionSheetItem},
};
use azul_appkit::l10n::{label, t};

use crate::{
    app::{AppState, Command},
    commands,
};

/// The rows of the sheet, by tag.
const SHOW: usize = 1;
const COLLAB: usize = 2;
const CLOSE: usize = 3;

/// What a row of the sheet asks for document `index`, whose collaboration is `collab` now.
#[must_use]
pub(crate) fn command_of(tag: usize, index: usize, collab: bool) -> Option<Command> {
    match tag {
        SHOW => Some(Command::Switch(index)),
        COLLAB => Some(Command::Collab(index, !collab)),
        CLOSE => Some(Command::Close(index)),
        _ => None,
    }
}

/// The key of the collaboration row's words: it offers the state it would switch to.
#[must_use]
pub(crate) const fn collab_key(on: bool) -> &'static str {
    if on {
        "azwriter-sheet-collab-off"
    } else {
        "azwriter-sheet-collab-on"
    }
}

/// The sheet over the frame, when a phone has one open.
#[must_use]
pub(crate) fn sheet(st: &AppState, app: &RefAny, phone: bool) -> Option<Dom> {
    let index = st.sheet_doc.filter(|_| phone)?;
    let doc = st.open.get(index)?;
    let sync = t(st.sync_of(index).key());
    let sheet = ActionSheet::create()
        .with_title(doc.title(), sync)
        .with_open(true)
        .with_on_event(app.clone(), on_sheet as ActionSheetOnEventCallbackType)
        .with_item(ActionSheetItem::create(label("azwriter-sheet-show"), "description", SHOW))
        .with_item(ActionSheetItem::create(label(collab_key(doc.collab)), "group_add", COLLAB))
        .with_item(ActionSheetItem::create(label("azwriter-sheet-close"), "close", CLOSE));
    Some(sheet.dom())
}

/// A row chosen runs its command on the document the sheet was opened for; a scrim tap closes the sheet.
extern "C" fn on_sheet(mut data: RefAny, mut info: CallbackInfo, event: ActionSheetEvent) -> Update {
    let asked = match data.downcast_mut::<AppState>() {
        Some(mut st) => {
            let index = st.sheet_doc.take();
            let collab = index.and_then(|i| st.open.get(i)).is_some_and(|d| d.collab);
            index.map(|i| (i, collab))
        }
        None => return Update::DoNothing,
    };
    let command = match (event.kind, asked) {
        (ActionSheetEventKind::ItemChosen, Some((index, collab))) => command_of(event.tag, index, collab),
        _ => None,
    };
    match command {
        Some(command) => commands::run(&mut data, command, &mut info),
        None => Update::RefreshDom,
    }
}

#[cfg(test)]
#[path = "ui_sheet_test.rs"]
mod ui_sheet_test;
