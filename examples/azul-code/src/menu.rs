//! The menu bar, VSCode's: File (Open Folder..., Open File..., Open Recent,
//! Save All, Close Editor, Close Folder), Edit (Undo, Redo, Find, Replace,
//! Find in Files), View (Command Palette..., Explorer, Search, Toggle Side
//! Bar, Terminal), Go (Go to File..., Go to Line...), Terminal (New
//! Terminal, Kill Terminal), Help (Keyboard Shortcuts, Settings, About).
//! Every item runs a command of [`crate::actions`].

use azul::{
    menu::{Menu, MenuItem, StringMenuItem},
    prelude::*,
};

use crate::{
    actions::{self, Action},
    app::AppState,
    commands, ui,
};

/// What a menu item runs.
struct ActionRef {
    app: RefAny,
    action: Action,
}

/// What an Open Recent item opens.
struct RecentItem {
    app: RefAny,
    index: usize,
}

/// An item running `action`.
fn item(app: &RefAny, action: Action) -> MenuItem {
    MenuItem::string(StringMenuItem::create(action.label()).with_callback(
        RefAny::new(ActionRef {
            app: app.clone(),
            action,
        }),
        on_menu_action,
    ))
}

/// A menu of the bar.
fn menu(title: &str, items: Vec<MenuItem>) -> MenuItem {
    MenuItem::string(StringMenuItem::create(title).with_children(items))
}

/// File > Open Recent: the recent folders (the newest first).
fn open_recent(app: &RefAny, st: &AppState) -> MenuItem {
    let mut items: Vec<MenuItem> = st
        .recent
        .iter()
        .enumerate()
        .map(|(index, folder)| {
            MenuItem::string(StringMenuItem::create(folder.as_str()).with_callback(
                RefAny::new(RecentItem {
                    app: app.clone(),
                    index,
                }),
                on_menu_recent,
            ))
        })
        .collect();
    if items.is_empty() {
        items.push(MenuItem::string(StringMenuItem::create("No Recent Folders")));
    }
    menu("Open Recent", items)
}

/// The window's menu bar.
#[must_use]
pub fn menu_bar(app: &RefAny, st: &AppState) -> Menu {
    Menu::create(vec![
        menu(
            "File",
            vec![
                item(app, Action::OpenFolder),
                item(app, Action::OpenFile),
                open_recent(app, st),
                item(app, Action::OpenSample),
                MenuItem::separator(),
                item(app, Action::Save),
                MenuItem::separator(),
                item(app, Action::CloseEditor),
                item(app, Action::CloseFolder),
            ],
        ),
        menu(
            "Edit",
            vec![
                item(app, Action::Undo),
                item(app, Action::Redo),
                MenuItem::separator(),
                item(app, Action::Find),
                item(app, Action::Replace),
                MenuItem::separator(),
                item(app, Action::FindInFiles),
            ],
        ),
        menu(
            "View",
            vec![
                item(app, Action::CommandPalette),
                MenuItem::separator(),
                item(app, Action::ShowExplorer),
                item(app, Action::FindInFiles),
                item(app, Action::ToggleSideBar),
                MenuItem::separator(),
                item(app, Action::ToggleTerminal),
            ],
        ),
        menu("Go", vec![item(app, Action::QuickOpen), item(app, Action::GoToLine)]),
        menu(
            "Terminal",
            vec![item(app, Action::NewTerminal), item(app, Action::KillTerminal)],
        ),
        menu(
            "Help",
            vec![
                item(app, Action::KeyboardShortcuts),
                item(app, Action::Settings),
                MenuItem::separator(),
                item(app, Action::About),
            ],
        ),
    ])
}

extern "C" fn on_menu_action(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, action)) = data.downcast_ref::<ActionRef>().map(|r| (r.app.clone(), r.action)) else {
        return Update::DoNothing;
    };
    println!("AZCODE_COMMAND {}", action.name());
    ui::with_state(&mut app, &mut info, |st, info, app| actions::run(st, info, app, action))
}

extern "C" fn on_menu_recent(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<RecentItem>().map(|r| (r.app.clone(), r.index)) else {
        return Update::DoNothing;
    };
    ui::with_state(&mut app, &mut info, |st, info, app| commands::open_recent(st, info, app, index))
}
