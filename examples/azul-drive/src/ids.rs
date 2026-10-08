//! Every DOM id and class AzDrive names, defined ONCE, each with the app's prefix `__azdrive_`
//! (the widgets' own names carry `__azul_`): the scripts and the theme find them by these names,
//! and no string literal repeats (wave-6 prefix ruling). Callers pass the constants as they are
//! (`.with_id(ids::INFO_BAR)`); `AzString::from_const_str` borrows the static bytes.

use azul::str::String as AzString;

use crate::model::ViewLayout;

// ==== ids ====

/// The InfoBar over the content (errors, notices).
pub const INFO_BAR: AzString = AzString::from_const_str("__azdrive_info_bar");
/// The content column (the InfoBar and the view).
pub const CONTENT: AzString = AzString::from_const_str("__azdrive_content");
/// The scrolling view of the place.
pub const VIEW: AzString = AzString::from_const_str("__azdrive_view");
/// The folder's rows: the virtual view under the Details header (every layout but the
/// IconGrid's) - its own DOM, which holds the rows in view and a screen either side.
pub const FOLDER_ROWS: AzString = AzString::from_const_str("__azdrive_folder_rows");
/// This PC's drive groups.
pub const THIS_PC: AzString = AzString::from_const_str("__azdrive_this_pc");
/// Quick access's pins and recent places.
pub const QUICK_ACCESS: AzString = AzString::from_const_str("__azdrive_quick_access");
/// The in-place rename field (F2, a new folder).
pub const RENAME_FIELD: AzString = AzString::from_const_str("__azdrive_rename_field");
/// The Details layout's column header row.
pub const DETAILS_HEADER: AzString = AzString::from_const_str("__azdrive_details_header");
/// "This folder is empty."
pub const EMPTY_FOLDER: AzString = AzString::from_const_str("__azdrive_empty_folder");
/// The folder's items in their layout.
pub const FOLDER_VIEW: AzString = AzString::from_const_str("__azdrive_folder_view");
/// The icon layouts' grid (azul's IconGrid); its items are `__azdrive_icon_grid-<index>`.
pub const ICON_GRID: AzString = AzString::from_const_str("__azdrive_icon_grid");

/// Explorer's chrome over the panes: the navigation row (Back, Forward, Up, the breadcrumb,
/// the search box) over the command bar.
pub const CHROME: AzString = AzString::from_const_str("__azdrive_chrome");
/// The command bar under the navigation row (the commands left, the panes right).
pub const COMMAND_BAR: AzString = AzString::from_const_str("__azdrive_command_bar");
/// The navigation pane: Finder's source list (its rows' box: Favorites, Locations, Cloud).
pub const NAV_PANE: AzString = AzString::from_const_str("__azdrive_nav_pane");

// ==== The source list (ui_sidebar.rs) ====

/// The source list's column (the rows, the activity area, the buttons under them).
pub const SIDEBAR: AzString = AzString::from_const_str("__azdrive_sidebar");
/// The section titles: FAVORITES, LOCATIONS, CLOUD (a click opens or closes the section).
pub const SIDE_FAVORITES: AzString = AzString::from_const_str("__azdrive_side_favorites");
pub const SIDE_LOCATIONS: AzString = AzString::from_const_str("__azdrive_side_locations");
pub const SIDE_CLOUD: AzString = AzString::from_const_str("__azdrive_side_cloud");
/// The rows of the places that are always there.
pub const SIDE_QUICK_ACCESS: AzString = AzString::from_const_str("__azdrive_side_quick_access");
pub const SIDE_THIS_PC: AzString = AzString::from_const_str("__azdrive_side_this_pc");
/// Cloud's "Add S3 drive" row.
pub const SIDE_ADD_DRIVE: AzString = AzString::from_const_str("__azdrive_side_add_drive");
/// The activity area (the transfers) and the buttons under the list.
pub const SIDE_ACTIVITY: AzString = AzString::from_const_str("__azdrive_side_activity");
pub const SIDE_ADD: AzString = AzString::from_const_str("__azdrive_side_add");
pub const SIDE_ACTIONS: AzString = AzString::from_const_str("__azdrive_side_actions");

/// A part of an id: the characters a script's `#id` selector takes as they are (letters,
/// digits, `-`, `_`), every other one as `_`.
fn id_part(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// A favourite's row by its folder's name: `__azdrive_side_fav_documents`.
#[must_use]
pub fn side_favorite(name: &str) -> AzString {
    AzString::from(format!("__azdrive_side_fav_{}", id_part(name)))
}

/// A pinned folder's row by its place in the pins: `__azdrive_side_pin_0`.
#[must_use]
pub fn side_pin(index: usize) -> AzString {
    AzString::from(format!("__azdrive_side_pin_{index}"))
}

/// A drive's row by its id: `__azdrive_side_drive_home`.
#[must_use]
pub fn side_drive(drive_id: &str) -> AzString {
    AzString::from(format!("__azdrive_side_drive_{}", id_part(drive_id)))
}

/// A drive's eject button by its id: `__azdrive_side_eject_<id>`.
#[must_use]
pub fn side_eject(drive_id: &str) -> AzString {
    AzString::from(format!("__azdrive_side_eject_{}", id_part(drive_id)))
}

// ==== The content's leaf and its foot ====

/// The leaf the view lies on (the InfoBar, the view, the path bar, the status line).
pub const LEAF: AzString = AzString::from_const_str("__azdrive_leaf");
/// Finder's path bar at the foot of the leaf, and the status line under it.
pub const PATH_BAR: AzString = AzString::from_const_str("__azdrive_path_bar");
pub const STATUS_LINE: AzString = AzString::from_const_str("__azdrive_status_line");

// ==== The command bar's tools (the ToolbarItem ids, so the DOM ids of the tools) ====

pub const CMD_NEW_FOLDER: AzString = AzString::from_const_str("__azdrive_cmd_new_folder");
pub const CMD_NEW_ITEM: AzString = AzString::from_const_str("__azdrive_cmd_new_item");
pub const CMD_CUT: AzString = AzString::from_const_str("__azdrive_cmd_cut");
pub const CMD_COPY: AzString = AzString::from_const_str("__azdrive_cmd_copy");
pub const CMD_PASTE: AzString = AzString::from_const_str("__azdrive_cmd_paste");
pub const CMD_RENAME: AzString = AzString::from_const_str("__azdrive_cmd_rename");
pub const CMD_DELETE: AzString = AzString::from_const_str("__azdrive_cmd_delete");
pub const CMD_UNDO: AzString = AzString::from_const_str("__azdrive_cmd_undo");
pub const CMD_PROPERTIES: AzString = AzString::from_const_str("__azdrive_cmd_properties");
pub const CMD_OPEN: AzString = AzString::from_const_str("__azdrive_cmd_open");
pub const CMD_UPLOAD: AzString = AzString::from_const_str("__azdrive_cmd_upload");
pub const CMD_DOWNLOAD: AzString = AzString::from_const_str("__azdrive_cmd_download");
pub const CMD_SORT: AzString = AzString::from_const_str("__azdrive_cmd_sort");
pub const CMD_LAYOUT_ICONS: AzString = AzString::from_const_str("__azdrive_cmd_layout_icons");
pub const CMD_LAYOUT_LIST: AzString = AzString::from_const_str("__azdrive_cmd_layout_list");
pub const CMD_LAYOUT_DETAILS: AzString = AzString::from_const_str("__azdrive_cmd_layout_details");
pub const CMD_SELECT_ALL: AzString = AzString::from_const_str("__azdrive_cmd_select_all");
pub const CMD_MORE: AzString = AzString::from_const_str("__azdrive_cmd_more");
pub const CMD_ADD_DRIVE: AzString = AzString::from_const_str("__azdrive_cmd_add_drive");
pub const CMD_ADD_FOLDER: AzString = AzString::from_const_str("__azdrive_cmd_add_folder");
pub const CMD_REMOVE_DRIVE: AzString = AzString::from_const_str("__azdrive_cmd_remove_drive");
pub const CMD_REFRESH: AzString = AzString::from_const_str("__azdrive_cmd_refresh");
pub const CMD_NAVIGATION_PANE: AzString =
    AzString::from_const_str("__azdrive_cmd_navigation_pane");
pub const CMD_PREVIEW_PANE: AzString = AzString::from_const_str("__azdrive_cmd_preview_pane");
pub const CMD_DETAILS_PANE: AzString = AzString::from_const_str("__azdrive_cmd_details_pane");
pub const CMD_OPTIONS: AzString = AzString::from_const_str("__azdrive_cmd_options");

/// The "Add drive" form and its fields.
pub const ADD_DRIVE: AzString = AzString::from_const_str("__azdrive_add_drive");
pub const ADD_NAME: AzString = AzString::from_const_str("__azdrive_add_name");
pub const ADD_ENDPOINT: AzString = AzString::from_const_str("__azdrive_add_endpoint");
pub const ADD_REGION: AzString = AzString::from_const_str("__azdrive_add_region");
pub const ADD_BUCKET: AzString = AzString::from_const_str("__azdrive_add_bucket");
pub const ADD_ACCESS_KEY: AzString = AzString::from_const_str("__azdrive_add_access_key");
pub const ADD_SECRET_KEY: AzString = AzString::from_const_str("__azdrive_add_secret_key");
pub const ADD_STATUS: AzString = AzString::from_const_str("__azdrive_add_status");
pub const ADD_ERROR: AzString = AzString::from_const_str("__azdrive_add_error");
/// The "delete for good?" question.
pub const CONFIRM_DELETE: AzString = AzString::from_const_str("__azdrive_confirm_delete");
/// "Move to / Copy to > Choose location" and its typed path.
pub const CHOOSE_LOCATION: AzString = AzString::from_const_str("__azdrive_choose_location");
pub const LOCATION_PATH: AzString = AzString::from_const_str("__azdrive_location_path");
/// "Replace or Skip Files" and its three answers.
pub const CONFLICT: AzString = AzString::from_const_str("__azdrive_conflict");
pub const CONFLICT_REPLACE: AzString = AzString::from_const_str("__azdrive_conflict_replace");
pub const CONFLICT_SKIP: AzString = AzString::from_const_str("__azdrive_conflict_skip");
pub const CONFLICT_KEEP_BOTH: AzString = AzString::from_const_str("__azdrive_conflict_keep_both");
/// The Properties dialog.
pub const PROPERTIES: AzString = AzString::from_const_str("__azdrive_properties");
/// The transfer queue.
pub const TRANSFERS: AzString = AzString::from_const_str("__azdrive_transfers");
/// A dialog shown as a sheet inside the window (`--dialogs inline`).
pub const SHEET: AzString = AzString::from_const_str("__azdrive_sheet");
/// The Options page and two of its controls.
pub const SETTINGS: AzString = AzString::from_const_str("__azdrive_settings");
pub const SETTING_LAYOUT: AzString = AzString::from_const_str("__azdrive_setting_layout");
pub const SETTING_START: AzString = AzString::from_const_str("__azdrive_setting_start");
/// The About page.
pub const ABOUT: AzString = AzString::from_const_str("__azdrive_about");

/// The preview pane and what it shows.
pub const PREVIEW_PANE: AzString = AzString::from_const_str("__azdrive_preview_pane");
pub const PREVIEW_TEXT: AzString = AzString::from_const_str("__azdrive_preview_text");
pub const PREVIEW_IMAGE: AzString = AzString::from_const_str("__azdrive_preview_image");
pub const PREVIEW_AUDIO: AzString = AzString::from_const_str("__azdrive_preview_audio");
pub const PREVIEW_PLAY: AzString = AzString::from_const_str("__azdrive_preview_play");
pub const PREVIEW_VIDEO: AzString = AzString::from_const_str("__azdrive_preview_video");
/// The details pane.
pub const DETAILS: AzString = AzString::from_const_str("__azdrive_details");

// ==== classes ====

/// A drive tile of This PC.
pub const DRIVE_CLASS: AzString = AzString::from_const_str("__azdrive_drive");
/// An item of a folder (any layout).
pub const ITEM_CLASS: AzString = AzString::from_const_str("__azdrive_item");
/// An item that is a folder (a drop target).
pub const FOLDER_CLASS: AzString = AzString::from_const_str("__azdrive_folder");
/// An item's name.
pub const NAME_CLASS: AzString = AzString::from_const_str("__azdrive_name");
/// A Details column header.
pub const COLUMN_CLASS: AzString = AzString::from_const_str("__azdrive_column");
/// The draggable edge of a Details column header.
pub const COLUMN_EDGE_CLASS: AzString = AzString::from_const_str("__azdrive_column_edge");
/// A picture drawn as a thumbnail.
pub const THUMBNAIL_CLASS: AzString = AzString::from_const_str("__azdrive_thumbnail");
/// A Details row on an odd line (Explorer's alternate shade).
pub const ROW_ALT_CLASS: AzString = AzString::from_const_str("__azdrive_row_alt");
/// A group's header line in a grouped folder view (a click opens or closes the group).
pub const GROUP_HEADER_CLASS: AzString = AzString::from_const_str("__azdrive_group_header");
/// The source list's column (what the keyboard finds it by).
pub const SIDEBAR_CLASS: AzString = AzString::from_const_str("__azdrive_source_list");
/// A row of the source list - a section title too: the arrow keys walk them in order.
pub const SIDE_ROW_CLASS: AzString = AzString::from_const_str("__azdrive_side_row");
/// A section title of the source list.
pub const SIDE_SECTION_CLASS: AzString = AzString::from_const_str("__azdrive_side_section");
/// The row of the place the window shows.
pub const SIDE_SELECTED_CLASS: AzString = AzString::from_const_str("__azdrive_side_selected");
/// A step of the path bar.
pub const CRUMB_CLASS: AzString = AzString::from_const_str("__azdrive_crumb");

/// The folder view's class for its layout: `__azdrive_layout_<name>` (the scripts read which
/// layout is showing from it).
pub const fn layout_class(layout: ViewLayout) -> AzString {
    match layout {
        ViewLayout::ExtraLargeIcons => {
            AzString::from_const_str("__azdrive_layout_extra_large_icons")
        }
        ViewLayout::LargeIcons => AzString::from_const_str("__azdrive_layout_large_icons"),
        ViewLayout::MediumIcons => AzString::from_const_str("__azdrive_layout_medium_icons"),
        ViewLayout::SmallIcons => AzString::from_const_str("__azdrive_layout_small_icons"),
        ViewLayout::List => AzString::from_const_str("__azdrive_layout_list"),
        ViewLayout::Details => AzString::from_const_str("__azdrive_layout_details"),
        ViewLayout::Tiles => AzString::from_const_str("__azdrive_layout_tiles"),
        ViewLayout::Content => AzString::from_const_str("__azdrive_layout_content"),
    }
}
