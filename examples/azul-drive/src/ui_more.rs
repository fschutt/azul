//! What File held before the location took its place: New tab, New window, Open terminal here,
//! Delete history, Keyboard shortcuts, Options, About and Close. They are behind the more button
//! (`more_horiz`) beside the location's pen - the widget's `LocationBar::with_more`, which reports
//! `MoreClicked` - as a menu under the button on a desktop and as the quick-action sheet on a
//! phone. The rows are data ([`rows`]) so that both are made of one list; a row that cannot run
//! now is greyed with the reason ([`why_not`]).
//!
//! The old File menu's Frequent places are not here: the sidebar's Favorites hold the pinned
//! folders and Quick access the places visited last.

use azul::{
    menu::{MenuItem, StringMenuItem},
    prelude::*,
};
use azul_appkit::l10n::label;

use crate::{
    actions::{self, menu_item, why_not, Action},
    DriveState,
};

/// One row of the menu behind the more button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Row {
    /// A command of the window.
    Item {
        /// A key of the resources.
        label: &'static str,
        /// A Material icon's name (the sheet draws it).
        icon: &'static str,
        action: Action,
    },
    /// A command with a few ways to run (Delete history: what it forgets): a submenu on a
    /// desktop, one row each on a phone.
    Group {
        label: &'static str,
        icon: &'static str,
        items: Vec<(&'static str, Action)>,
    },
    /// A line between two groups of rows (a menu only).
    Separator,
}

/// The rows, in the order File had them.
#[must_use]
pub(crate) fn rows() -> Vec<Row> {
    let item = |label, icon, action| Row::Item { label, icon, action };
    vec![
        item("azdrive-loctab-new", "tab", Action::NewTab),
        item("azdrive-file-new-window", "open_in_new", Action::NewWindow),
        Row::Group {
            label: "azdrive-file-terminal-here",
            icon: "terminal",
            items: vec![
                ("azdrive-file-terminal", Action::OpenTerminal { azterm: false }),
                ("azdrive-file-azterm", Action::OpenTerminal { azterm: true }),
            ],
        },
        Row::Separator,
        Row::Group {
            label: "azdrive-file-delete-history",
            icon: "history",
            items: vec![
                (
                    "azdrive-file-recent-places",
                    Action::ClearHistory {
                        recent: true,
                        back_forward: false,
                    },
                ),
                (
                    "azdrive-file-back-forward",
                    Action::ClearHistory {
                        recent: false,
                        back_forward: true,
                    },
                ),
                (
                    "azdrive-file-everything",
                    Action::ClearHistory {
                        recent: true,
                        back_forward: true,
                    },
                ),
            ],
        },
        Row::Separator,
        item("azdrive-file-shortcuts", "keyboard", Action::Shortcuts),
        item("azdrive-backstage-options", "tune", Action::Options),
        item("azdrive-file-about", "info", Action::About),
        Row::Separator,
        item("azdrive-ribbon-close", "close", Action::CloseWindow),
    ]
}

/// The menu under the more button of a desktop's location.
#[must_use]
pub(crate) fn menu_items(s: &DriveState, app: &RefAny) -> Vec<MenuItem> {
    let greyed = |action: &Action| why_not(s, action).is_some();
    rows()
        .into_iter()
        .map(|row| match row {
            Row::Item { label, action, .. } => {
                let disabled = greyed(&action);
                menu_item(app, label, action, disabled)
            }
            Row::Group { label: text, items, .. } => MenuItem::String(
                StringMenuItem::create(label(text)).with_children(
                    items
                        .into_iter()
                        .map(|(text, action)| {
                            let disabled = greyed(&action);
                            menu_item(app, text, action, disabled)
                        })
                        .collect::<Vec<MenuItem>>(),
                ),
            ),
            Row::Separator => MenuItem::Separator,
        })
        .collect()
}

/// One row of the phone's sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SheetRow {
    pub label: &'static str,
    pub icon: &'static str,
    pub action: Action,
}

/// The rows of the phone's sheet: the same list flat - a group's ways one row each, the lines
/// left out.
#[must_use]
pub(crate) fn sheet_rows() -> Vec<SheetRow> {
    let mut out = Vec::new();
    for row in rows() {
        match row {
            Row::Item { label, icon, action } => out.push(SheetRow { label, icon, action }),
            Row::Group { icon, items, .. } => {
                out.extend(items.into_iter().map(|(label, action)| SheetRow { label, icon, action }));
            }
            Row::Separator => {}
        }
    }
    out
}

/// The more button was clicked: a desktop's menu opens under it; a phone's sheet opens.
pub(crate) fn open(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    if s.phone {
        s.sheet = Some(crate::ui_sheet::Sheet::File);
    } else {
        actions::open_menu_below(info, menu_items(s, app));
    }
}

#[cfg(test)]
#[path = "ui_more_test.rs"]
mod ui_more_test;
