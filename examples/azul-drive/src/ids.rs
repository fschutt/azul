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
/// The "Load more" button under a long listing.
pub const LOAD_MORE: AzString = AzString::from_const_str("__azdrive_load_more");
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
/// A dialog shown as a sheet inside the window (`AZDRIVE_DIALOGS=inline`).
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
