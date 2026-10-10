//! The menu bar, VSCode's: File (Open Folder..., Open File..., Open Recent,
//! Save All, Close Editor, Close Folder), Edit (Undo, Redo, Find, Replace,
//! Find in Files), View (Command Palette..., Explorer, Search, Toggle Side
//! Bar, Terminal), Go (Go to File..., Go to Line...), Terminal (New
//! Terminal, Kill Terminal), Help (Keyboard Shortcuts, Settings, About).
//! Every item runs a command of [`crate::actions`].
//!
//! Every item carries the app itself and a callback of its own (one per
//! command, one per recent folder): the window re-applies its menu bar only
//! when the menu's hash changes, and a fresh `RefAny` per item would change
//! it on every rebuild of the window - every key typed.

use azul::{
    callbacks::CallbackType,
    menu::{Menu, MenuItem, StringMenuItem},
    prelude::*,
};

use azul_appkit::l10n::label;

use crate::{
    actions::{self, Action},
    app::AppState,
    commands, ui,
    workspace::RECENT_MAX,
};

/// A command run from the menu (`AZCODE_COMMAND <name>`).
fn run_from_menu(data: &mut RefAny, info: &mut CallbackInfo, action: Action) -> Update {
    println!("AZCODE_COMMAND {}", action.name());
    ui::with_state(data, info, |st, info, app| actions::run(st, info, app, action))
}

/// Recent folder `index` opened from File > Open Recent.
fn open_recent(data: &mut RefAny, info: &mut CallbackInfo, index: usize) -> Update {
    ui::with_state(data, info, |st, info, app| commands::open_recent(st, info, app, index))
}

/// One callback per command: `fn(app, info)` running it.
macro_rules! action_callbacks {
    ($($name:ident => $action:ident),* $(,)?) => {
        $(
            extern "C" fn $name(mut data: RefAny, mut info: CallbackInfo) -> Update {
                run_from_menu(&mut data, &mut info, Action::$action)
            }
        )*

        /// The callback of `action`.
        fn callback_of(action: Action) -> CallbackType {
            match action {
                $(Action::$action => $name,)*
            }
        }
    };
}

action_callbacks! {
    on_open_folder => OpenFolder,
    on_open_file => OpenFile,
    on_open_sample => OpenSample,
    on_save => Save,
    on_close_editor => CloseEditor,
    on_close_folder => CloseFolder,
    on_undo => Undo,
    on_redo => Redo,
    on_find => Find,
    on_replace => Replace,
    on_find_in_files => FindInFiles,
    on_command_palette => CommandPalette,
    on_show_explorer => ShowExplorer,
    on_toggle_side_bar => ToggleSideBar,
    on_toggle_terminal => ToggleTerminal,
    on_quick_open => QuickOpen,
    on_go_to_line => GoToLine,
    on_new_terminal => NewTerminal,
    on_kill_terminal => KillTerminal,
    on_refresh_explorer => RefreshExplorer,
    on_collapse_folders => CollapseFolders,
    on_settings => Settings,
    on_keyboard_shortcuts => KeyboardShortcuts,
    on_about => About,
}

/// One callback per place in the recent list.
macro_rules! recent_callbacks {
    ($($name:ident => $index:literal),* $(,)?) => {
        $(
            extern "C" fn $name(mut data: RefAny, mut info: CallbackInfo) -> Update {
                open_recent(&mut data, &mut info, $index)
            }
        )*

        /// The callbacks of the recent list's places, in order.
        const RECENT_CALLBACKS: [CallbackType; RECENT_MAX] = [$($name),*];
    };
}

recent_callbacks! {
    on_recent_0 => 0,
    on_recent_1 => 1,
    on_recent_2 => 2,
    on_recent_3 => 3,
    on_recent_4 => 4,
    on_recent_5 => 5,
    on_recent_6 => 6,
    on_recent_7 => 7,
    on_recent_8 => 8,
    on_recent_9 => 9,
}

/// An item running `action`.
fn item(app: &RefAny, action: Action) -> MenuItem {
    MenuItem::string(StringMenuItem::create(label(action.label())).with_callback(app.clone(), callback_of(action)))
}

/// A menu of the bar.
fn menu(title: &str, items: Vec<MenuItem>) -> MenuItem {
    MenuItem::string(StringMenuItem::create(label(title)).with_children(items))
}

/// File > Open Recent: the recent folders (the newest first).
fn open_recent_menu(app: &RefAny, st: &AppState) -> MenuItem {
    let mut items: Vec<MenuItem> = st
        .recent
        .iter()
        .zip(RECENT_CALLBACKS)
        .map(|(folder, callback)| {
            MenuItem::string(StringMenuItem::create(folder.as_str()).with_callback(app.clone(), callback))
        })
        .collect();
    if items.is_empty() {
        items.push(MenuItem::string(StringMenuItem::create(label("azcode-no-recent"))));
    }
    menu("azcode-open-recent", items)
}

/// The window's menu bar.
#[must_use]
pub fn menu_bar(app: &RefAny, st: &AppState) -> Menu {
    Menu::create(vec![
        menu(
            "azcode-menu-file",
            vec![
                item(app, Action::OpenFolder),
                item(app, Action::OpenFile),
                open_recent_menu(app, st),
                item(app, Action::OpenSample),
                MenuItem::separator(),
                item(app, Action::Save),
                MenuItem::separator(),
                item(app, Action::CloseEditor),
                item(app, Action::CloseFolder),
            ],
        ),
        menu(
            "azcode-menu-edit",
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
            "azcode-menu-view",
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
        menu("azcode-menu-go", vec![item(app, Action::QuickOpen), item(app, Action::GoToLine)]),
        menu(
            "azcode-action-terminal",
            vec![item(app, Action::NewTerminal), item(app, Action::KillTerminal)],
        ),
        menu(
            "azcode-menu-help",
            vec![
                item(app, Action::KeyboardShortcuts),
                item(app, Action::Settings),
                MenuItem::separator(),
                item(app, Action::About),
            ],
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_has_a_menu_callback_of_its_own() {
        let mut seen: Vec<usize> = Action::ALL.iter().map(|a| callback_of(*a) as usize).collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), Action::ALL.len());
        assert_eq!(RECENT_CALLBACKS.len(), RECENT_MAX);
    }
}
