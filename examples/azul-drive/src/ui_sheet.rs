//! The phone's quick-action sheet (azul's `ActionSheet`): a long press on a row of the folder view
//! opens it over the window with the row's name and the actions of its context menu - Preview,
//! Open (Download on a cloud drive), Rename, Move to, Copy, Properties and, in red, Delete; a long
//! press on a row of the source list opens it with that row's menu (Open, Properties, the drive's
//! keys and sync, Remove drive), and the location's more button opens it with what File held (`ui_more`). The window keeps what the sheet is for (`DriveState::sheet`);
//! a choice closes the sheet and runs the action as the menu would have.

use azul::{
    callbacks::ActionSheetOnEventCallbackType,
    prelude::*,
    str::String as AzString,
    widgets::{ActionSheet, ActionSheetEvent, ActionSheetEventKind, ActionSheetItem},
};
use azul_appkit::l10n::label;

use crate::{
    actions::{self, why_not, Action},
    browse::Entry,
    fileops::TransferKind,
    ui_more, ui_overlays, ui_sidebar, with_state, DriveState,
};

/// What a long press opened the sheet on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Sheet {
    /// A row of the folder view: its key.
    Item(String),
    /// A row of the source list: its tag (its place in the rows).
    Place(usize),
    /// What File held, behind the location's more button (`ui_more`).
    File,
}

/// What a row of an item's sheet does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ItemAct {
    /// The preview of the item, large (Quick Look).
    Preview,
    /// One of the window's actions on the item.
    Do(Action),
}

/// One row of an item's sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ItemRow {
    /// A key of the resources.
    pub label: &'static str,
    /// A Material icon's name.
    pub icon: &'static str,
    pub act: ItemAct,
    /// Deleting: shown in red.
    pub danger: bool,
}

/// The rows of `entry`'s sheet, in the order of the context menu: Preview, Open (a file of a cloud
/// drive downloads), Rename, Move to, Copy, Properties and Delete last, in red. A folder has no
/// download.
#[must_use]
pub(crate) fn item_rows(entry: &Entry, cloud: bool) -> Vec<ItemRow> {
    let row = |label, icon, act, danger| ItemRow {
        label,
        icon,
        act,
        danger,
    };
    let open = if cloud && !entry.is_folder {
        row("azdrive-menu-download", "download", ItemAct::Do(Action::Download), false)
    } else {
        row("azdrive-menu-open", "open_in_new", ItemAct::Do(Action::Open), false)
    };
    vec![
        row("azdrive-sheet-preview", "visibility", ItemAct::Preview, false),
        open,
        row(
            "azdrive-menu-rename",
            "edit",
            ItemAct::Do(Action::Rename),
            false,
        ),
        row(
            "azdrive-ribbon-move-to",
            "drive_file_move",
            ItemAct::Do(Action::ChooseLocation(TransferKind::Move)),
            false,
        ),
        row("azdrive-menu-copy", "content_copy", ItemAct::Do(Action::Copy), false),
        row(
            "azdrive-menu-properties",
            "info",
            ItemAct::Do(Action::Properties),
            false,
        ),
        row("azdrive-sheet-trash", "delete", ItemAct::Do(Action::Delete), true),
    ]
}

/// The sheet over the window while one is open (`None`: none, or the row it was for is gone).
pub(crate) fn sheet(s: &DriveState, app: &RefAny) -> Option<Dom> {
    let which = s.sheet.as_ref()?;
    let mut sheet = ActionSheet::create()
        .with_form(s.form)
        .with_open(true)
        .with_on_event(app.clone(), on_sheet as ActionSheetOnEventCallbackType);
    match which {
        Sheet::Item(key) => {
            let entry = s.entry(key)?;
            let cloud = s.current_drive_id().is_some_and(|id| !s.is_local_drive(&id));
            sheet = sheet.with_title(
                AzString::from(entry.name.as_str()),
                AzString::from(entry.kind()),
            );
            for (tag, row) in item_rows(entry, cloud).iter().enumerate() {
                let mut item = ActionSheetItem::create(label(row.label), AzString::from(row.icon), tag)
                    .with_danger(row.danger);
                if let ItemAct::Do(action) = &row.act {
                    if let Some(why) = why_not(s, action) {
                        item = item.with_disabled_reason(label(&why));
                    }
                }
                sheet = sheet.with_item(item);
            }
        }
        Sheet::File => {
            sheet = sheet.with_title(label("azdrive-tab-file"), AzString::from(""));
            for (tag, row) in ui_more::sheet_rows().iter().enumerate() {
                let mut item =
                    ActionSheetItem::create(label(row.label), AzString::from(row.icon), tag);
                if let Some(why) = why_not(s, &row.action) {
                    item = item.with_disabled_reason(label(&why));
                }
                sheet = sheet.with_item(item);
            }
        }
        Sheet::Place(tag) => {
            let (name, entries) = ui_sidebar::place_sheet(s, *tag)?;
            sheet = sheet.with_title(label(&name), AzString::from(""));
            for (tag, entry) in entries.iter().enumerate() {
                sheet = sheet.with_item(
                    ActionSheetItem::create(label(&entry.label), AzString::from(entry.icon), tag)
                        .with_danger(entry.danger),
                );
            }
        }
    }
    Some(sheet.dom())
}

/// The sheet's report: a row chosen (the sheet closes, the action runs), or the scrim, the grab
/// handle or Escape (it closes).
extern "C" fn on_sheet(mut data: RefAny, mut info: CallbackInfo, event: ActionSheetEvent) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let which = s.sheet.take();
        if event.kind != ActionSheetEventKind::ItemChosen {
            return;
        }
        let Some(which) = which else {
            return;
        };
        match which {
            Sheet::Item(key) => run_item(info, app, s, &key, event.tag),
            Sheet::Place(tag) => ui_sidebar::run_place_sheet(info, app, s, tag, event.tag),
            Sheet::File => {
                if let Some(row) = ui_more::sheet_rows().into_iter().nth(event.tag) {
                    println!("AZDRIVE_SHEET {}", row.label);
                    actions::run_action(info, app, s, row.action);
                }
            }
        }
    })
}

/// Row `choice` of item `key`'s sheet runs: the item is the selection first, as the context menu
/// makes it.
fn run_item(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    key: &str,
    choice: usize,
) {
    let Some(entry) = s.entry(key).cloned() else {
        return;
    };
    let cloud = s.current_drive_id().is_some_and(|id| !s.is_local_drive(&id));
    let Some(row) = item_rows(&entry, cloud).into_iter().nth(choice) else {
        return;
    };
    if !s.selection.contains(key) {
        actions::click_item(info, app, s, key, false, false);
    }
    println!("AZDRIVE_SHEET {}", row.label);
    match row.act {
        ItemAct::Preview => ui_overlays::toggle_quick_look(info, app, s),
        ItemAct::Do(action) => actions::run_action(info, app, s, action),
    }
}

/// A long press on the folder view's row `key` (a phone): the row is the selection, and the
/// sheet opens on it.
pub(crate) fn open_for_item(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, key: &str) {
    if s.entry(key).is_none() {
        return;
    }
    if !s.selection.contains(key) {
        actions::click_item(info, app, s, key, false, false);
    }
    s.sheet = Some(Sheet::Item(key.to_string()));
    println!("AZDRIVE_SHEET open {key}");
}

#[cfg(test)]
#[path = "ui_sheet_test.rs"]
mod ui_sheet_test;
