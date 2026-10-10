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
/// The search's results: their column header (Name, Folder, Match, Date modified, Size).
pub const FIND_HEADER: AzString = AzString::from_const_str("__azdrive_find_header");
/// The note over a cloud drive's results: searched by name, slower.
pub const FIND_NOTE: AzString = AzString::from_const_str("__azdrive_find_note");
/// "Searching..." / "No items match your search." where the results would be.
pub const FIND_EMPTY: AzString = AzString::from_const_str("__azdrive_find_empty");

/// Explorer's address row under the ribbon (Back, Forward, Recent, Up, the breadcrumb box with
/// Refresh, the search box).
pub const CHROME: AzString = AzString::from_const_str("__azdrive_chrome");
/// Windows 8's ribbon (its tab strip is the window's title bar; File drops the File menu).
pub const RIBBON: AzString = AzString::from_const_str("__azdrive_ribbon");
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
/// Cloud's "Add drive" row.
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

/// A cloud drive's state glyph by its id (a synced drive's says its status line):
/// `__azdrive_side_sync_<id>`.
#[must_use]
pub fn side_sync_state(drive_id: &str) -> AzString {
    AzString::from(format!("__azdrive_side_sync_{}", id_part(drive_id)))
}

// ==== The content's leaf and its foot ====

/// The leaf the view lies on (the InfoBar, the view, the path bar, the status line).
pub const LEAF: AzString = AzString::from_const_str("__azdrive_leaf");
/// Finder's path bar at the foot of the leaf, and the status line under it.
pub const PATH_BAR: AzString = AzString::from_const_str("__azdrive_path_bar");
pub const STATUS_LINE: AzString = AzString::from_const_str("__azdrive_status_line");

/// The "Add drive" dialog (ui_add_drive.rs): its content on every page, the pages, their
/// controls. A source's row is `__azdrive_add_service_<source id>`, a form's field
/// `__azdrive_add_field_<key>`, a tier `__azdrive_add_tier_<index>` (the functions below).
pub const ADD_DRIVE: AzString = AzString::from_const_str("__azdrive_add_drive");
pub const ADD_CHOOSE: AzString = AzString::from_const_str("__azdrive_add_choose");
pub const ADD_BUY: AzString = AzString::from_const_str("__azdrive_add_buy");
pub const ADD_SOURCES: AzString = AzString::from_const_str("__azdrive_add_sources");
pub const ADD_FORM: AzString = AzString::from_const_str("__azdrive_add_form");
pub const ADD_CHOICE_BUY: AzString = AzString::from_const_str("__azdrive_add_choice_buy");
pub const ADD_CHOICE_CONNECT: AzString = AzString::from_const_str("__azdrive_add_choice_connect");
pub const ADD_BACK: AzString = AzString::from_const_str("__azdrive_add_back");
/// The drive's name (a form's, and Buy storage's).
pub const ADD_NAME: AzString = AzString::from_const_str("__azdrive_add_name");
pub const ADD_TEST: AzString = AzString::from_const_str("__azdrive_add_test");
/// A consumer cloud's "Sign in" (Google Drive, Dropbox, OneDrive) and its status line.
pub const ADD_SIGN_IN: AzString = AzString::from_const_str("__azdrive_add_sign_in");
pub const ADD_SIGN_IN_STATUS: AzString = AzString::from_const_str("__azdrive_add_sign_in_status");
pub const ADD_SAVE: AzString = AzString::from_const_str("__azdrive_add_save");
pub const ADD_CANCEL: AzString = AzString::from_const_str("__azdrive_add_cancel");
pub const ADD_YEARLY: AzString = AzString::from_const_str("__azdrive_add_yearly");
pub const ADD_CREATE_TEST: AzString = AzString::from_const_str("__azdrive_add_create_test");
pub const ADD_BUY_BUTTON: AzString = AzString::from_const_str("__azdrive_add_buy_button");
pub const ADD_STOP: AzString = AzString::from_const_str("__azdrive_add_stop");
pub const ADD_RETRY: AzString = AzString::from_const_str("__azdrive_add_retry");
pub const ADD_STATUS: AzString = AzString::from_const_str("__azdrive_add_status");
pub const ADD_ERROR: AzString = AzString::from_const_str("__azdrive_add_error");

/// A source's row of the Add drive dialog: `__azdrive_add_service_webdav`.
#[must_use]
pub fn add_service(id: &str) -> AzString {
    AzString::from(format!("__azdrive_add_service_{}", id_part(id)))
}

/// A form's field: `__azdrive_add_field_endpoint`.
#[must_use]
pub fn add_field(key: &str) -> AzString {
    AzString::from(format!("__azdrive_add_field_{}", id_part(key)))
}

/// A path field's "Choose..." button: `__azdrive_add_choose_root`.
#[must_use]
pub fn add_choose(key: &str) -> AzString {
    AzString::from(format!("__azdrive_add_choose_{}", id_part(key)))
}

/// A tier of Buy storage: `__azdrive_add_tier_0`.
#[must_use]
pub fn add_tier(index: usize) -> AzString {
    AzString::from(format!("__azdrive_add_tier_{index}"))
}

/// Buy storage's payment: the country, the provider switch of a pill, the price line, the
/// consent, "Check again" and "Open the page again"; a pill is `__azdrive_add_pill_<method>`
/// (`card`, `sepa_debit`, ...).
pub const ADD_COUNTRY: AzString = AzString::from_const_str("__azdrive_add_country");
pub const ADD_PROVIDER: AzString = AzString::from_const_str("__azdrive_add_provider");
pub const ADD_PRICE: AzString = AzString::from_const_str("__azdrive_add_price");
pub const ADD_CONSENT: AzString = AzString::from_const_str("__azdrive_add_consent");
pub const ADD_CHECK_AGAIN: AzString = AzString::from_const_str("__azdrive_add_check_again");
pub const ADD_OPEN_AGAIN: AzString = AzString::from_const_str("__azdrive_add_open_again");

/// A payment pill of Buy storage: `__azdrive_add_pill_card`.
#[must_use]
pub fn add_pill(method: &str) -> AzString {
    AzString::from(format!("__azdrive_add_pill_{}", id_part(method)))
}

/// The payment popover (its own window, a `<transient-window>` of the dialog): its content, the
/// verified-origin chip and its host, the card artwork and its brand, the cardholder name, the
/// provider's page in the web view, Pay, "Open in browser instead", the provider's message.
pub const PAY_POPOVER: AzString = AzString::from_const_str("__azdrive_pay_popover");
pub const PAY_CHIP: AzString = AzString::from_const_str("__azdrive_pay_chip");
pub const PAY_HOST: AzString = AzString::from_const_str("__azdrive_pay_host");
pub const PAY_CARD: AzString = AzString::from_const_str("__azdrive_pay_card");
pub const PAY_BRAND: AzString = AzString::from_const_str("__azdrive_pay_brand");
pub const PAY_NAME: AzString = AzString::from_const_str("__azdrive_pay_name");
pub const PAY_WEBVIEW: AzString = AzString::from_const_str("__azdrive_pay_webview");
pub const PAY_CONFIRM: AzString = AzString::from_const_str("__azdrive_pay_confirm");
pub const PAY_BROWSER: AzString = AzString::from_const_str("__azdrive_pay_browser");
pub const PAY_NOTICE: AzString = AzString::from_const_str("__azdrive_pay_notice");
/// The web view's marker: how Pay's callback (in the popover's window) finds the web view to
/// tell the fields page to confirm.
pub const PAY_WEBVIEW_MARKER: &str = "azdrive-pay-webview";
/// The "delete for good?" question.
pub const CONFIRM_DELETE: AzString = AzString::from_const_str("__azdrive_confirm_delete");
/// "Move to / Copy to > Choose location" and its typed path.
pub const CHOOSE_LOCATION: AzString = AzString::from_const_str("__azdrive_choose_location");
pub const LOCATION_PATH: AzString = AzString::from_const_str("__azdrive_location_path");
/// The bar over the drive in view while a recovery-key lockdown of it is pending, and its
/// Cancel.
pub const LOCKDOWN_BAR: AzString = AzString::from_const_str("__azdrive_lockdown_bar");
pub const LOCKDOWN_CANCEL: AzString = AzString::from_const_str("__azdrive_lockdown_cancel");
/// The voucher dialog of a drive (Options > Drives): its code and Redeem.
pub const VOUCHER: AzString = AzString::from_const_str("__azdrive_voucher");
pub const VOUCHER_CODE: AzString = AzString::from_const_str("__azdrive_voucher_code");
pub const VOUCHER_REDEEM: AzString = AzString::from_const_str("__azdrive_voucher_redeem");
/// Add drive > Buy storage's "I have a voucher", its code and Redeem.
pub const ADD_VOUCHER: AzString = AzString::from_const_str("__azdrive_add_voucher");
pub const ADD_VOUCHER_CODE: AzString = AzString::from_const_str("__azdrive_add_voucher_code");
pub const ADD_VOUCHER_REDEEM: AzString =
    AzString::from_const_str("__azdrive_add_voucher_redeem");

/// A banned drive (ban contract v1): its banner, the banner's text, Copy everything, the closed
/// drive's message.
pub const BAN_BAR: AzString = AzString::from_const_str("__azdrive_ban_bar");
pub const BAN_TEXT: AzString = AzString::from_const_str("__azdrive_ban_text");
pub const BAN_COPY: AzString = AzString::from_const_str("__azdrive_ban_copy");
pub const BAN_CLOSED: AzString = AzString::from_const_str("__azdrive_ban_closed");

/// Cash by post: Add drive's "Pick up a paid drive with a claim code" (the first page's
/// button, the code's box, Pick up), the posted order's waiting line and claim code, its pages'
/// buttons.
pub const ADD_CHOICE_CLAIM: AzString = AzString::from_const_str("__azdrive_add_choice_claim");
pub const ADD_CLAIM_CODE: AzString = AzString::from_const_str("__azdrive_add_claim_code");
pub const ADD_PICK_UP: AzString = AzString::from_const_str("__azdrive_add_pick_up");
pub const ADD_CASH_WAITING: AzString = AzString::from_const_str("__azdrive_add_cash_waiting");
pub const ADD_CASH_CLAIM_CODE: AzString =
    AzString::from_const_str("__azdrive_add_cash_claim_code");
pub const CASH_COPY_SAVE: AzString = AzString::from_const_str("__azdrive_cash_copy_save");
pub const CASH_COPY_PRINT: AzString = AzString::from_const_str("__azdrive_cash_copy_print");
pub const CASH_SLIP_SAVE: AzString = AzString::from_const_str("__azdrive_cash_slip_save");
pub const CASH_SLIP_PRINT: AzString = AzString::from_const_str("__azdrive_cash_slip_print");
/// The drive list's cash orders, and each one's parts: `__azdrive_side_cash_<n>_<line|copy|slip|
/// dismiss>`.
pub const SIDE_CASH: AzString = AzString::from_const_str("__azdrive_side_cash");
#[must_use]
pub fn side_cash(index: usize, what: &str) -> AzString {
    AzString::from(format!("__azdrive_side_cash_{index}_{what}"))
}

/// "Restore as of..." of a drive: its dialog, the time and Restore.
pub const RESTORE: AzString = AzString::from_const_str("__azdrive_restore");
pub const RESTORE_TIME: AzString = AzString::from_const_str("__azdrive_restore_time");
pub const RESTORE_GO: AzString = AzString::from_const_str("__azdrive_restore_go");
/// Options > Drives' "Restore as of..." of a drive: `__azdrive_restore_<id>`.
#[must_use]
pub fn restore_button(drive_id: &str) -> AzString {
    AzString::from(format!("__azdrive_restore_{}", id_part(drive_id)))
}

/// Options > Drives' "Redeem a voucher" of a drive: `__azdrive_voucher_<id>`.
#[must_use]
pub fn voucher_button(drive_id: &str) -> AzString {
    AzString::from(format!("__azdrive_voucher_{}", id_part(drive_id)))
}
/// "Replace or Skip Files" and its three answers.
pub const CONFLICT: AzString = AzString::from_const_str("__azdrive_conflict");
pub const CONFLICT_REPLACE: AzString = AzString::from_const_str("__azdrive_conflict_replace");
pub const CONFLICT_SKIP: AzString = AzString::from_const_str("__azdrive_conflict_skip");
pub const CONFLICT_KEEP_BOTH: AzString = AzString::from_const_str("__azdrive_conflict_keep_both");
/// The folder sync's dialogs: pairing a drive with a folder (the folder here, the drive's
/// folder, Sync), a conflict (D52: keep mine, take theirs, keep both); its Options section.
pub const SYNC_PAIR: AzString = AzString::from_const_str("__azdrive_sync_pair");
pub const SYNC_FOLDER: AzString = AzString::from_const_str("__azdrive_sync_folder");
pub const SYNC_PREFIX: AzString = AzString::from_const_str("__azdrive_sync_prefix");
pub const SYNC_PAIR_OK: AzString = AzString::from_const_str("__azdrive_sync_pair_ok");
pub const SYNC_CONFLICT: AzString = AzString::from_const_str("__azdrive_sync_conflict");
pub const SYNC_KEEP_MINE: AzString = AzString::from_const_str("__azdrive_sync_keep_mine");
pub const SYNC_TAKE_THEIRS: AzString = AzString::from_const_str("__azdrive_sync_take_theirs");
pub const SYNC_KEEP_BOTH: AzString = AzString::from_const_str("__azdrive_sync_keep_both");
pub const SYNC_OPTIONS: AzString = AzString::from_const_str("__azdrive_sync_options");
pub const SYNC_DELETE: AzString = AzString::from_const_str("__azdrive_sync_delete");
pub const SYNC_DELETE_OK: AzString = AzString::from_const_str("__azdrive_sync_delete_ok");
/// The burst guard's question, "I was hacked", the mass delete's question.
pub const SYNC_BURST: AzString = AzString::from_const_str("__azdrive_sync_burst");
pub const SYNC_BURST_MINE: AzString = AzString::from_const_str("__azdrive_sync_burst_mine");
pub const SYNC_BURST_HACKED: AzString = AzString::from_const_str("__azdrive_sync_burst_hacked");
pub const SYNC_HACKED: AzString = AzString::from_const_str("__azdrive_sync_hacked");
pub const SYNC_HACKED_LOCKDOWN: AzString =
    AzString::from_const_str("__azdrive_sync_hacked_lockdown");
pub const SYNC_HACKED_RESTORE: AzString = AzString::from_const_str("__azdrive_sync_hacked_restore");
pub const SYNC_MASS: AzString = AzString::from_const_str("__azdrive_sync_mass");
pub const SYNC_MASS_KEEP: AzString = AzString::from_const_str("__azdrive_sync_mass_keep");
pub const SYNC_MASS_DELETE: AzString = AzString::from_const_str("__azdrive_sync_mass_delete");
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
/// A search result's Folder cell (where it is).
pub const FIND_FOLDER_CLASS: AzString = AzString::from_const_str("__azdrive_find_folder");
/// A search result's Match cell (the line its contents matched on).
pub const FIND_MATCH_CLASS: AzString = AzString::from_const_str("__azdrive_find_match");
/// The sync state icon after an item's name (its accessible name says the state).
pub const SYNC_STATE_CLASS: AzString = AzString::from_const_str("__azdrive_sync_state");
/// The search index's overlay after an item's name: indexed, or not indexable.
pub const INDEX_STATE_CLASS: AzString = AzString::from_const_str("__azdrive_index_state");

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

// ==== An encrypted drive's recovery methods (recovery.rs) ====

/// The emergency kit's buttons on the recovery sheet: Print, Save as PDF, Save to a USB stick.
pub const KIT_PRINT: AzString = AzString::from_const_str("__azdrive_kit_print");
pub const KIT_SAVE: AzString = AzString::from_const_str("__azdrive_kit_save");
pub const KIT_USB: AzString = AzString::from_const_str("__azdrive_kit_usb");
/// The recovery sheet's boxes of the groups typed back: `__azdrive_sheet_group_<n>` (0-based,
/// in the order asked).
#[must_use]
pub fn sheet_group(slot: usize) -> AzString {
    AzString::from(format!("__azdrive_sheet_group_{slot}"))
}
/// The recovery sheet's "I have written it down".
pub const SHEET_DONE: AzString = AzString::from_const_str("__azdrive_sheet_done");
/// A drill's code, "Later" and "Check".
pub const DRILL_CODE: AzString = AzString::from_const_str("__azdrive_drill_code");
pub const DRILL_LATER: AzString = AzString::from_const_str("__azdrive_drill_later");
pub const DRILL_CHECK: AzString = AzString::from_const_str("__azdrive_drill_check");
/// Trusted contacts, the owner's "Add": the code, each person's name and contact key, Make.
pub const CONTACTS_CODE: AzString = AzString::from_const_str("__azdrive_contacts_code");
pub const CONTACTS_MAKE: AzString = AzString::from_const_str("__azdrive_contacts_make");
#[must_use]
pub fn contacts_name(slot: usize) -> AzString {
    AzString::from(format!("__azdrive_contacts_name_{slot}"))
}
#[must_use]
pub fn contacts_key(slot: usize) -> AzString {
    AzString::from(format!("__azdrive_contacts_key_{slot}"))
}
/// The shares made: a sealed share's text (by row), Done.
#[must_use]
pub fn contacts_share(row: usize) -> AzString {
    AzString::from(format!("__azdrive_contacts_share_{row}"))
}
pub const CONTACTS_DONE: AzString = AzString::from_const_str("__azdrive_contacts_done");
/// A contact's side: the key made, the text pasted and Continue, the safety number, an answer
/// for a held share (by its place in the list), the reply.
pub const CONTACT_KEY_TEXT: AzString = AzString::from_const_str("__azdrive_contact_key_text");
pub const CONTACT_PASTE: AzString = AzString::from_const_str("__azdrive_contact_paste");
pub const CONTACT_PASTE_OK: AzString = AzString::from_const_str("__azdrive_contact_paste_ok");
pub const SAFETY_NUMBER: AzString = AzString::from_const_str("__azdrive_safety_number");
#[must_use]
pub fn contact_answer(index: usize) -> AzString {
    AzString::from(format!("__azdrive_contact_answer_{index}"))
}
pub const CONTACT_REPLY: AzString = AzString::from_const_str("__azdrive_contact_reply");
/// Options > Drives > Shares you hold for others: its three doors.
pub const CONTACT_BE: AzString = AzString::from_const_str("__azdrive_contact_be");
pub const CONTACT_TAKE: AzString = AzString::from_const_str("__azdrive_contact_take");
pub const CONTACT_HELP: AzString = AzString::from_const_str("__azdrive_contact_help");
/// Recover with trusted contacts: the request, the two answers' boxes, Recover; the code back.
pub const CONTACTS_REQUEST: AzString = AzString::from_const_str("__azdrive_contacts_request");
#[must_use]
pub fn contacts_answer_box(slot: usize) -> AzString {
    AzString::from(format!("__azdrive_contacts_answer_{slot}"))
}
pub const CONTACTS_RECOVER: AzString = AzString::from_const_str("__azdrive_contacts_recover");
pub const REBUILT_CODE: AzString = AzString::from_const_str("__azdrive_rebuilt_code");
/// Options > Drives' Recovery section, a drive's warning below two methods, a method's button:
/// `__azdrive_method_<drive>_<code|contacts|devices|passkey>_<test|add|remove|count>`.
pub const RECOVERY_METHODS: AzString = AzString::from_const_str("__azdrive_recovery_methods");
#[must_use]
pub fn method_warning(drive_id: &str) -> AzString {
    AzString::from(format!("__azdrive_method_warning_{}", id_part(drive_id)))
}
#[must_use]
pub fn method_button(
    drive_id: &str,
    method: crate::recovery_health::Method,
    action: crate::recovery_health::MethodAction,
) -> AzString {
    use crate::recovery_health::{Method, MethodAction};
    let method = match method {
        Method::Code => "code",
        Method::Contacts => "contacts",
        Method::OtherDevice => "devices",
        Method::Passkey => "passkey",
    };
    let action = match action {
        MethodAction::Test => "test",
        MethodAction::Add => "add",
        MethodAction::Remove => "remove",
        MethodAction::CountAgain => "count",
    };
    AzString::from(format!("__azdrive_method_{}_{method}_{action}", id_part(drive_id)))
}
/// A printed share's paper buttons on the shares made: `__azdrive_share_<print|save|usb>_<row>`.
#[must_use]
pub fn share_paper(row: usize, what: &str) -> AzString {
    AzString::from(format!("__azdrive_share_{what}_{row}"))
}
/// "Unlock with the recovery code": the code's box.
pub const UNLOCK_CODE: AzString = AzString::from_const_str("__azdrive_unlock_code");
