//! Explorer's command bar, under the navigation row (Windows 7's, and 11's):
//!
//! ```text
//! [New folder][+] | [Cut][Copy][Paste][Rename][Delete][undo] | [Properties] | [Sort]
//!     [icons][list][details] | [select all][...]          [nav][preview][details][options]
//! ```
//!
//! At the left the commands of the open place - a folder's New folder, New item, Cut, Copy,
//! Paste, Rename, Delete, Undo and Properties (Upload and Download on a cloud drive), This
//! PC's drive commands, Quick access's Open - then Sort, the three views (Large icons, List,
//! Details), Select all and "See more" (...) with every other command of the window. At the
//! right end the panes (Navigation, Preview, Details) and the Options. Two azul `Toolbar`s:
//! the left one moves what does not fit into its own "more" menu when the window is narrow.
//! Every tool runs its [`Action`] (the tool's id names it) or is greyed with the reason.

use azul::{
    callbacks::ToolbarOnEventCallbackType,
    prelude::*,
    str::String as AzString,
    widgets::{Toolbar, ToolbarEvent, ToolbarEventKind, ToolbarItem},
};

use crate::{
    actions::{self, why_not, Action, Toggle},
    browse::Place,
    ids,
    model::ViewLayout,
    with_state, DriveState,
};

/// What the right end takes: four icon-only tools (the toolbar's own estimate of one is
/// 34 px) and the gaps around the two bars.
const RIGHT_TOOLS_PX: f32 = 4.0 * 34.0 + 32.0;

/// What tool `id` runs.
fn command_of(id: &str) -> Option<Action> {
    let is = |tool: &AzString| tool.as_str() == id;
    let action = if is(&ids::CMD_NEW_FOLDER) {
        Action::NewFolder
    } else if is(&ids::CMD_NEW_ITEM) {
        Action::NewItemMenu
    } else if is(&ids::CMD_CUT) {
        Action::Cut
    } else if is(&ids::CMD_COPY) {
        Action::Copy
    } else if is(&ids::CMD_PASTE) {
        Action::Paste
    } else if is(&ids::CMD_RENAME) {
        Action::Rename
    } else if is(&ids::CMD_DELETE) {
        Action::Delete
    } else if is(&ids::CMD_UNDO) {
        Action::Undo
    } else if is(&ids::CMD_PROPERTIES) {
        Action::Properties
    } else if is(&ids::CMD_OPEN) {
        Action::Open
    } else if is(&ids::CMD_UPLOAD) {
        Action::Upload
    } else if is(&ids::CMD_DOWNLOAD) {
        Action::Download
    } else if is(&ids::CMD_SORT) {
        Action::SortMenu
    } else if is(&ids::CMD_LAYOUT_ICONS) {
        Action::SetLayout(ViewLayout::LargeIcons)
    } else if is(&ids::CMD_LAYOUT_LIST) {
        Action::SetLayout(ViewLayout::List)
    } else if is(&ids::CMD_LAYOUT_DETAILS) {
        Action::SetLayout(ViewLayout::Details)
    } else if is(&ids::CMD_SELECT_ALL) {
        Action::SelectAll
    } else if is(&ids::CMD_MORE) {
        Action::MoreMenu
    } else if is(&ids::CMD_ADD_DRIVE) {
        Action::AddDrive
    } else if is(&ids::CMD_ADD_FOLDER) {
        Action::AddLocalDrive
    } else if is(&ids::CMD_REMOVE_DRIVE) {
        Action::RemoveDrive
    } else if is(&ids::CMD_REFRESH) {
        Action::Refresh
    } else if is(&ids::CMD_NAVIGATION_PANE) {
        Action::Toggle(Toggle::NavigationPane)
    } else if is(&ids::CMD_PREVIEW_PANE) {
        Action::Toggle(Toggle::PreviewPane)
    } else if is(&ids::CMD_DETAILS_PANE) {
        Action::Toggle(Toggle::DetailsPane)
    } else if is(&ids::CMD_OPTIONS) {
        Action::Options
    } else {
        return None;
    };
    Some(action)
}

/// A tool running what its id names; `labelled` shows the label beside the icon (else it is
/// the tooltip). Greyed, with the reason, when the command cannot run now.
fn tool(s: &DriveState, id: AzString, label: &str, icon: &str, labelled: bool) -> ToolbarItem {
    let reason = command_of(id.as_str()).and_then(|action| why_not(s, &action));
    let item = ToolbarItem::create_button(id, label, icon).with_show_label(labelled);
    match reason {
        Some(reason) => item.with_disabled(reason),
        None => item,
    }
}

/// A tool that is on or off (a view, a pane), icon-only.
fn switch(id: AzString, label: &str, icon: &str, on: bool) -> ToolbarItem {
    ToolbarItem::create_toggle(id, label, icon, on)
}

/// A folder's commands: Windows 7's New folder and the clipboard and organize commands, with
/// Upload and Download on a cloud drive.
fn folder_tools(s: &DriveState, items: &mut Vec<ToolbarItem>) {
    let undo = s
        .undo
        .last()
        .map_or_else(|| String::from("Undo"), crate::UndoOp::label);
    items.extend([
        tool(s, ids::CMD_NEW_FOLDER, "New folder", "create_new_folder", true),
        tool(s, ids::CMD_NEW_ITEM, "New item", "note_add", false),
        ToolbarItem::create_separator(),
        tool(s, ids::CMD_CUT, "Cut", "content_cut", true),
        tool(s, ids::CMD_COPY, "Copy", "content_copy", true),
        tool(s, ids::CMD_PASTE, "Paste", "content_paste", true),
        tool(s, ids::CMD_RENAME, "Rename", "drive_file_rename_outline", true),
        tool(s, ids::CMD_DELETE, "Delete", "delete", true),
        tool(s, ids::CMD_UNDO, &undo, "undo", false),
        ToolbarItem::create_separator(),
        tool(s, ids::CMD_PROPERTIES, "Properties", "info", true),
    ]);
    let cloud = s
        .current_drive_id()
        .is_some_and(|id| !s.is_local_drive(&id));
    if cloud {
        items.extend([
            ToolbarItem::create_separator(),
            tool(s, ids::CMD_UPLOAD, "Upload", "upload", true),
            tool(s, ids::CMD_DOWNLOAD, "Download", "download", true),
        ]);
    }
}

/// This PC's commands: the drives (Windows' "Map network drive" is an S3 drive here).
fn this_pc_tools(s: &DriveState, items: &mut Vec<ToolbarItem>) {
    items.extend([
        tool(s, ids::CMD_ADD_DRIVE, "Add S3 drive", "cloud", true),
        tool(s, ids::CMD_ADD_FOLDER, "Add folder", "create_new_folder", true),
        tool(s, ids::CMD_REMOVE_DRIVE, "Remove drive", "remove_circle", true),
        ToolbarItem::create_separator(),
        tool(s, ids::CMD_OPEN, "Open", "open_in_new", true),
        tool(s, ids::CMD_PROPERTIES, "Properties", "info", true),
        tool(s, ids::CMD_REFRESH, "Refresh", "refresh", false),
    ]);
}

/// Quick access's commands.
fn quick_access_tools(s: &DriveState, items: &mut Vec<ToolbarItem>) {
    items.extend([
        tool(s, ids::CMD_OPEN, "Open", "open_in_new", true),
        tool(s, ids::CMD_PROPERTIES, "Properties", "info", true),
    ]);
}

/// The left bar: the place's commands, Sort, the three views, Select all, See more.
fn commands(s: &DriveState, available: f32) -> Toolbar {
    let mut items = Vec::new();
    match &s.place {
        Place::Folder { .. } => folder_tools(s, &mut items),
        Place::ThisPc => this_pc_tools(s, &mut items),
        Place::QuickAccess => quick_access_tools(s, &mut items),
    }
    let layout = s.settings.layout;
    items.extend([
        ToolbarItem::create_separator(),
        tool(s, ids::CMD_SORT, "Sort", "sort", true),
        switch(
            ids::CMD_LAYOUT_ICONS,
            "Large icons",
            "grid_view",
            layout == ViewLayout::LargeIcons,
        ),
        switch(ids::CMD_LAYOUT_LIST, "List", "view_list", layout == ViewLayout::List),
        switch(
            ids::CMD_LAYOUT_DETAILS,
            "Details",
            "view_headline",
            layout == ViewLayout::Details,
        ),
        ToolbarItem::create_separator(),
        tool(s, ids::CMD_SELECT_ALL, "Select all", "select_all", false),
        tool(s, ids::CMD_MORE, "See more", "more_horiz", false).with_never_overflow(true),
    ]);
    Toolbar::create("Commands")
        .with_items(items)
        .with_available_width(available)
}

/// The right end: the panes and the Options.
fn panes(s: &DriveState) -> Toolbar {
    let settings = &s.settings;
    Toolbar::create("Panes")
        .with_item(switch(
            ids::CMD_NAVIGATION_PANE,
            "Navigation pane",
            "vertical_split",
            settings.navigation_pane,
        ))
        .with_item(switch(
            ids::CMD_PREVIEW_PANE,
            "Preview pane (Alt+P)",
            "preview",
            settings.preview_pane,
        ))
        .with_item(switch(
            ids::CMD_DETAILS_PANE,
            "Details pane (Alt+Shift+P)",
            "view_sidebar",
            settings.details_pane,
        ))
        .with_item(tool(s, ids::CMD_OPTIONS, "Options", "settings", false))
}

/// The command bar of a window `width` px wide: the commands at the left, the panes at the
/// right end.
pub(crate) fn command_bar(s: &DriveState, app: &RefAny, width: f32) -> Dom {
    let available = (width - RIGHT_TOOLS_PX).max(160.0);
    Dom::create_div()
        .with_id(ids::COMMAND_BAR)
        .with_css(
            "display: flex; flex-direction: row; align-items: center; \
             justify-content: space-between; padding: 2px 8px 3px 8px; min-width: 0px; \
             border-bottom: 1px solid system:separator;",
        )
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; min-width: 0px; overflow: hidden;")
                .with_child(
                    commands(s, available)
                        .with_on_event(app.clone(), on_command as ToolbarOnEventCallbackType)
                        .dom(),
                ),
        )
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; flex-shrink: 0;")
                .with_child(
                    panes(s)
                        .with_on_event(app.clone(), on_command as ToolbarOnEventCallbackType)
                        .dom(),
                ),
        )
}

/// A tool was pressed (or picked from a bar's "more" menu): the tool's id names the command.
extern "C" fn on_command(mut data: RefAny, mut info: CallbackInfo, event: ToolbarEvent) -> Update {
    if event.kind == ToolbarEventKind::Choose {
        return Update::DoNothing;
    }
    let Some(action) = command_of(event.id.as_str()) else {
        return Update::DoNothing;
    };
    with_state(&mut data, &mut info, |info, app, s| {
        actions::run_action(info, app, s, action)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every tool of the bars names a command, and no two tools name the same id.
    #[test]
    fn every_tool_id_runs_a_command() {
        let all = [
            ids::CMD_NEW_FOLDER,
            ids::CMD_NEW_ITEM,
            ids::CMD_CUT,
            ids::CMD_COPY,
            ids::CMD_PASTE,
            ids::CMD_RENAME,
            ids::CMD_DELETE,
            ids::CMD_UNDO,
            ids::CMD_PROPERTIES,
            ids::CMD_OPEN,
            ids::CMD_UPLOAD,
            ids::CMD_DOWNLOAD,
            ids::CMD_SORT,
            ids::CMD_LAYOUT_ICONS,
            ids::CMD_LAYOUT_LIST,
            ids::CMD_LAYOUT_DETAILS,
            ids::CMD_SELECT_ALL,
            ids::CMD_MORE,
            ids::CMD_ADD_DRIVE,
            ids::CMD_ADD_FOLDER,
            ids::CMD_REMOVE_DRIVE,
            ids::CMD_REFRESH,
            ids::CMD_NAVIGATION_PANE,
            ids::CMD_PREVIEW_PANE,
            ids::CMD_DETAILS_PANE,
            ids::CMD_OPTIONS,
        ];
        let names: Vec<&str> = all.iter().map(|id| id.as_str()).collect();
        for name in &names {
            assert!(command_of(name).is_some(), "{name} runs nothing");
            assert!(name.starts_with("__azdrive_cmd_"), "{name}: the app's prefix");
        }
        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "an id named twice");
        assert_eq!(command_of("__azdrive_cmd_nothing"), None);
        assert_eq!(
            command_of(ids::CMD_LAYOUT_ICONS.as_str()),
            Some(Action::SetLayout(ViewLayout::LargeIcons))
        );
    }
}
