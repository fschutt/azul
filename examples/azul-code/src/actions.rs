//! The window's commands: ONE table the menu bar, the command palette
//! (Mod+Shift+P) and the keys run through - VSCode's "File: Open
//! Folder...", "View: Terminal", "Go: Go to File..." and the rest.
//!
//! On stdout: `AZCODE_COMMAND <name>` for a command run from the menu or
//! the palette.

use azul::prelude::*;
use azul_appkit::ui as kit;

use crate::{
    app::{AppState, Palette, PaletteKind, Side},
    commands, terminal,
};

/// A command of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    OpenFolder,
    OpenFile,
    OpenSample,
    Save,
    CloseEditor,
    CloseFolder,
    Undo,
    Redo,
    Find,
    Replace,
    FindInFiles,
    CommandPalette,
    ShowExplorer,
    ToggleSideBar,
    ToggleTerminal,
    QuickOpen,
    GoToLine,
    NewTerminal,
    KillTerminal,
    RefreshExplorer,
    CollapseFolders,
    Settings,
    KeyboardShortcuts,
    About,
}

impl Action {
    /// Every command, in the order the command palette lists them.
    pub const ALL: [Action; 24] = [
        Action::OpenFolder,
        Action::OpenFile,
        Action::OpenSample,
        Action::Save,
        Action::CloseEditor,
        Action::CloseFolder,
        Action::Undo,
        Action::Redo,
        Action::Find,
        Action::Replace,
        Action::FindInFiles,
        Action::CommandPalette,
        Action::ShowExplorer,
        Action::ToggleSideBar,
        Action::ToggleTerminal,
        Action::QuickOpen,
        Action::GoToLine,
        Action::NewTerminal,
        Action::KillTerminal,
        Action::RefreshExplorer,
        Action::CollapseFolders,
        Action::Settings,
        Action::KeyboardShortcuts,
        Action::About,
    ];

    /// What the menu and the palette call it: a key of the resources.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Action::OpenFolder => "azcode-action-open-folder",
            Action::OpenFile => "azcode-action-open-file",
            Action::OpenSample => "azcode-action-open-sample",
            Action::Save => "azcode-action-save-all",
            Action::CloseEditor => "azcode-action-close-editor",
            Action::CloseFolder => "azcode-action-close-folder",
            Action::Undo => "azcode-action-undo",
            Action::Redo => "azcode-action-redo",
            Action::Find => "azcode-action-find",
            Action::Replace => "azcode-action-replace",
            Action::FindInFiles => "azcode-action-find-in-files",
            Action::CommandPalette => "azcode-action-command-palette",
            Action::ShowExplorer => "azcode-action-explorer",
            Action::ToggleSideBar => "azcode-action-toggle-side-bar",
            Action::ToggleTerminal => "azcode-action-terminal",
            Action::QuickOpen => "azcode-action-quick-open",
            Action::GoToLine => "azcode-action-go-to-line",
            Action::NewTerminal => "azcode-action-new-terminal",
            Action::KillTerminal => "azcode-action-kill-terminal",
            Action::RefreshExplorer => "azcode-action-refresh-explorer",
            Action::CollapseFolders => "azcode-action-collapse-folders",
            Action::Settings => "azcode-action-settings",
            Action::KeyboardShortcuts => "azcode-action-keyboard-shortcuts",
            Action::About => "azcode-action-about",
        }
    }

    /// The palette's category ("File: Open Folder..."): a key of the resources.
    #[must_use]
    pub fn category(self) -> &'static str {
        match self {
            Action::OpenFolder
            | Action::OpenFile
            | Action::OpenSample
            | Action::Save
            | Action::CloseEditor
            | Action::CloseFolder => "azcode-menu-file",
            Action::Undo | Action::Redo | Action::Find | Action::Replace => "azcode-menu-edit",
            Action::FindInFiles => "azcode-category-search",
            Action::CommandPalette
            | Action::ShowExplorer
            | Action::ToggleSideBar
            | Action::ToggleTerminal => "azcode-menu-view",
            Action::QuickOpen | Action::GoToLine => "azcode-menu-go",
            Action::NewTerminal | Action::KillTerminal => "azcode-action-terminal",
            Action::RefreshExplorer | Action::CollapseFolders => "azcode-action-explorer",
            Action::Settings => "azcode-category-preferences",
            Action::KeyboardShortcuts | Action::About => "azcode-menu-help",
        }
    }

    /// Its keys (`Mod` is Cmd on macOS, Ctrl elsewhere; a space between
    /// the two keys of a chord); "" for none.
    #[must_use]
    pub fn keys(self) -> &'static str {
        match self {
            Action::OpenFolder => "Mod+K Mod+O",
            Action::Save => "Mod+S",
            Action::CloseEditor => "Mod+W",
            Action::Undo => "Mod+Z",
            Action::Redo => "Mod+Shift+Z",
            Action::Find => "Mod+F",
            Action::Replace => "Mod+H",
            Action::FindInFiles => "Mod+Shift+F",
            Action::CommandPalette => "Mod+Shift+P",
            Action::ShowExplorer => "Mod+Shift+E",
            Action::ToggleSideBar => "Mod+B",
            Action::ToggleTerminal => "Ctrl+`",
            Action::QuickOpen => "Mod+P",
            Action::GoToLine => "Mod+G",
            Action::NewTerminal => "Ctrl+Shift+`",
            Action::Settings => "Mod+,",
            Action::KeyboardShortcuts => "F1",
            Action::OpenFile
            | Action::OpenSample
            | Action::CloseFolder
            | Action::KillTerminal
            | Action::RefreshExplorer
            | Action::CollapseFolders
            | Action::About => "",
        }
    }

    /// Its name on stdout (`AZCODE_COMMAND toggle-terminal`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Action::OpenFolder => "open-folder",
            Action::OpenFile => "open-file",
            Action::OpenSample => "open-sample",
            Action::Save => "save",
            Action::CloseEditor => "close-editor",
            Action::CloseFolder => "close-folder",
            Action::Undo => "undo",
            Action::Redo => "redo",
            Action::Find => "find",
            Action::Replace => "replace",
            Action::FindInFiles => "find-in-files",
            Action::CommandPalette => "command-palette",
            Action::ShowExplorer => "show-explorer",
            Action::ToggleSideBar => "toggle-side-bar",
            Action::ToggleTerminal => "toggle-terminal",
            Action::QuickOpen => "quick-open",
            Action::GoToLine => "go-to-line",
            Action::NewTerminal => "new-terminal",
            Action::KillTerminal => "kill-terminal",
            Action::RefreshExplorer => "refresh-explorer",
            Action::CollapseFolders => "collapse-folders",
            Action::Settings => "settings",
            Action::KeyboardShortcuts => "keyboard-shortcuts",
            Action::About => "about",
        }
    }

    /// Its icon in the palette.
    #[must_use]
    pub fn icon(self) -> &'static str {
        match self {
            Action::OpenFolder | Action::OpenSample => "folder_open",
            Action::OpenFile => "file_open",
            Action::Save => "save",
            Action::CloseEditor | Action::CloseFolder => "close",
            Action::Undo => "undo",
            Action::Redo => "redo",
            Action::Find | Action::Replace | Action::FindInFiles => "search",
            Action::CommandPalette => "keyboard",
            Action::ShowExplorer => "content_copy",
            Action::ToggleSideBar => "view_sidebar",
            Action::ToggleTerminal | Action::NewTerminal => "terminal",
            Action::KillTerminal => "delete",
            Action::QuickOpen => "description",
            Action::GoToLine => "format_list_numbered",
            Action::RefreshExplorer => "refresh",
            Action::CollapseFolders => "unfold_less",
            Action::Settings => "settings",
            Action::KeyboardShortcuts => "keyboard",
            Action::About => "info",
        }
    }
}

/// Runs `action` (from the menu, the palette or a key).
pub fn run(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, action: Action) {
    let has_file = st.tabs.active().is_some();
    match action {
        Action::OpenFolder => commands::ask_folder(app),
        Action::OpenFile => commands::ask_file(app),
        Action::OpenSample => commands::open_sample(st, info, app),
        Action::Save => commands::save(st, info, app),
        Action::CloseEditor => commands::close_tab(st),
        Action::CloseFolder => commands::close_folder(st),
        Action::Undo | Action::Redo => {
            if let Some(doc) = st.tabs.active_mut() {
                doc.undo_redo(action == Action::Redo);
            }
            if st.find.open {
                st.refresh_find();
            }
        }
        Action::Find if has_file => commands::open_find(st, info, false),
        Action::Replace if has_file => commands::open_find(st, info, true),
        Action::Find | Action::Replace => {}
        Action::FindInFiles => commands::show_search(st, info),
        Action::CommandPalette => {
            st.palette = Some(Palette {
                kind: PaletteKind::Commands,
                query: String::new(),
            });
            commands::focus_soon(info, crate::ids::COMMAND_PALETTE.as_str());
        }
        Action::ShowExplorer => {
            st.side = Side::Explorer;
            st.side_visible = true;
        }
        Action::ToggleSideBar => st.side_visible = !st.side_visible,
        Action::ToggleTerminal => terminal::toggle(st, info, app),
        Action::QuickOpen => commands::quick_open(st, info, app),
        Action::GoToLine if has_file => {
            st.goto = Some(String::new());
            commands::focus_soon(info, crate::ids::GOTO_INPUT.as_str());
        }
        Action::GoToLine => {}
        Action::NewTerminal => terminal::new_terminal(st, info, app),
        Action::KillTerminal => terminal::kill_terminal(st, info),
        Action::RefreshExplorer => commands::refresh_explorer(st, info, app),
        Action::CollapseFolders => {
            if let Some(w) = st.workspace.as_mut() {
                w.collapse_all();
            }
        }
        Action::Settings => kit::open_settings(&st.kit, None),
        Action::KeyboardShortcuts => kit::open_settings(&st.kit, Some("Shortcuts")),
        Action::About => kit::open_settings(&st.kit, Some("About")),
    }
}

/// The commands the palette lists now (a command that has nothing to act
/// on - Close Folder without a folder - is left out).
#[must_use]
pub fn available(st: &AppState) -> Vec<Action> {
    let has_file = st.tabs.active().is_some();
    let has_folder = st.workspace.is_some();
    let has_shell = !st.panel.terminals.is_empty();
    Action::ALL
        .into_iter()
        .filter(|a| match a {
            Action::CloseEditor
            | Action::Undo
            | Action::Redo
            | Action::Find
            | Action::Replace
            | Action::GoToLine => has_file,
            Action::CloseFolder
            | Action::QuickOpen
            | Action::RefreshExplorer
            | Action::CollapseFolders => has_folder,
            Action::KillTerminal => has_shell,
            Action::CommandPalette => false,
            _ => true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_has_a_label_a_category_and_a_name_of_its_own() {
        let mut names: Vec<&str> = Action::ALL.iter().map(|a| a.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Action::ALL.len(), "no two commands share a name");
        for a in Action::ALL {
            assert!(!a.label().is_empty() && !a.category().is_empty() && !a.icon().is_empty(), "{a:?}");
        }
        assert_eq!(Action::OpenFolder.keys(), "Mod+K Mod+O", "VSCode's chord");
        assert_eq!(Action::ToggleTerminal.keys(), "Ctrl+`");
    }
}
