//! Windows 8 File Explorer's ribbon over Finder's body, on azul's Ribbon widget (Office 2010's
//! control kinds: large and small buttons, split buttons, small items three to a column, a
//! gallery, check boxes in a group):
//!
//! ```text
//! File | Home | Share | View                  (at This PC: File | Computer | View)
//! Home:  Clipboard [Copy][Paste] Cut / Copy path / Paste shortcut
//!        Organize  [Move to v][Copy to v][Delete|v][Rename] Undo
//!        New       [New folder] New item v / Easy access v / Add drive
//!        Open      [Properties|v] Open|v / Edit / History
//!        Select    Select all / Select none / Invert selection
//! Share: Send      [Email][Zip] Print / Burn to disc / Fax
//!        Cloud     [Upload][Download][Copy link]
//!        Share with [Advanced security]
//! View:  Panes     [Navigation pane] Preview pane / Details pane
//!        Layout    the eight layouts (a gallery)
//!        Current view [Sort by v] Group by v / Add columns v / Size all columns to fit
//!        Show/hide Item check boxes / File name extensions / Hidden items [Hide selected items]
//!        Options   [Options]
//! Computer: Location [Properties][Open]  Network [Add drive][Add folder as drive]
//!        [Remove drive]  System [Refresh][Options]
//! Search (Search Tools, while a search is open): Location [Current folder][All subfolders]
//!        Refine [Date modified v] Kind v / Size v  Options [File contents] Hidden items /
//!        Skip ignored files / Open file location / Index this drive / Index files in the
//!        cloud  Saved [Save search]
//!        Saved searches v  Close [Close search]
//! ```
//!
//! Every control runs its [`Action`] or is greyed with the reason it cannot run now
//! ([`why_not`]): what AzDrive cannot do at all (a Windows shortcut, a disc burner) says so. The
//! tab strip IS the window's title bar (`with_tabs_in_titlebar`: the window drags around the
//! tabs, which sit clear of macOS's traffic lights); the window's title is the open place's
//! path. File drops Windows 8's File menu - a mini backstage in a popup under its tab (azul's
//! `RibbonFileMenu`): Open new window, Open terminal here, Delete history, Help, Close, and the
//! frequent places with their pins.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType,
        RibbonFileMenuOnEventCallbackType, RibbonGalleryOnSelectCallbackType,
        RibbonOnTabClickCallbackType,
    },
    prelude::*,
    str::String as AzString,
    vec::{RibbonFileMenuCommandVec, RibbonFileMenuPlaceVec, RibbonGalleryCellVec},
    widgets::{
        CheckBoxState, Ribbon, RibbonAppButton, RibbonArrow, RibbonButton, RibbonFileMenu,
        RibbonFileMenuCommand, RibbonFileMenuEvent, RibbonFileMenuEventKind,
        RibbonFileMenuPlace, RibbonGallery, RibbonGalleryCell, RibbonItem, RibbonRow, RibbonTab,
    },
};
use azul_appkit::{
    l10n::{label, t, t_args, t_label},
    ribbon::{self as ribbon_kit, group, RibbonCommand},
};

use crate::{
    actions::{self, action_ref, on_action, why_not, Action, ActionRef, Toggle},
    browse::{self, Place},
    ids,
    model::ViewLayout,
    sync_view::SyncAction,
    with_state, DriveState,
};

// ==== The tabs ====

/// A tab of AzDrive's ribbon (File is the application button, not a tab).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum RibbonTabKind {
    #[default]
    Home,
    Share,
    View,
    /// This PC's tab (Windows 8's "azdrive-tab-computer"): the drives.
    Computer,
    /// Windows 8's Search tab (Search Tools), while a search of a folder is open.
    Search,
}

impl RibbonTabKind {
    /// Its name on the tab strip, a key of AzDrive's resources.
    pub(crate) fn key(self) -> &'static str {
        match self {
            RibbonTabKind::Home => "azdrive-tab-home",
            RibbonTabKind::Share => "azdrive-tab-share",
            RibbonTabKind::View => "azdrive-tab-view",
            RibbonTabKind::Computer => "azdrive-tab-computer",
            RibbonTabKind::Search => "azdrive-tab-search",
        }
    }

    /// Its name on stdout (`AZDRIVE_RIBBON_TAB`), English whatever the window's language.
    pub(crate) fn label(self) -> &'static str {
        match self {
            RibbonTabKind::Home => "Home",
            RibbonTabKind::Share => "Share",
            RibbonTabKind::View => "View",
            RibbonTabKind::Computer => "Computer",
            RibbonTabKind::Search => "Search",
        }
    }
}

/// The tabs a place shows, as Explorer 8 shows them: a folder (and Quick access) Home, Share and
/// View - and Search while a search of the folder is open (`searching`); This PC Computer and
/// View.
pub(crate) fn tabs_of(place: &Place, searching: bool) -> &'static [RibbonTabKind] {
    match place {
        Place::ThisPc if searching => &[
            RibbonTabKind::Computer,
            RibbonTabKind::View,
            RibbonTabKind::Search,
        ],
        Place::ThisPc => &[RibbonTabKind::Computer, RibbonTabKind::View],
        Place::Folder { .. } if searching => &[
            RibbonTabKind::Home,
            RibbonTabKind::Share,
            RibbonTabKind::View,
            RibbonTabKind::Search,
        ],
        _ => &[RibbonTabKind::Home, RibbonTabKind::Share, RibbonTabKind::View],
    }
}

/// The index of the tab the ribbon shows: the one chosen last where the place has it (View
/// stays View from a folder to This PC), else the place's first.
pub(crate) fn active_index(place: &Place, searching: bool, chosen: RibbonTabKind) -> usize {
    tabs_of(place, searching)
        .iter()
        .position(|t| *t == chosen)
        .unwrap_or(0)
}

// ==== The controls ====

/// Every ribbon button runs an [`Action`] through `on_action` (azul-appkit's ribbon builder).
impl RibbonCommand for Action {
    fn click_data(self, app: &RefAny) -> RefAny {
        action_ref(app, self)
    }

    fn on_click() -> ButtonOnClickCallbackType {
        on_action
    }
}

/// A button running `action`, greyed with the reason when it cannot run now.
fn button(s: &DriveState, app: &RefAny, icon: &str, text: &str, action: Action) -> RibbonButton {
    let reason = why_not(s, &action);
    let b = ribbon_kit::button(app, icon, text, action);
    match reason {
        Some(reason) => b.with_disabled(label(&reason)),
        None => b,
    }
}

/// A button whose whole face opens a menu (`action` opens it).
fn menu_button(s: &DriveState, app: &RefAny, icon: &str, label: &str, action: Action) -> RibbonButton {
    button(s, app, icon, label, action).with_arrow(RibbonArrow::Menu)
}

/// A split button: its face runs `action`, its arrow opens `menu` (Office's Paste, Explorer's
/// Delete and Properties).
fn split_button(
    s: &DriveState,
    app: &RefAny,
    icon: &str,
    label: &str,
    action: Action,
    menu: Action,
) -> RibbonButton {
    button(s, app, icon, label, action)
        .with_on_arrow_click(action_ref(app, menu), on_action as ButtonOnClickCallbackType)
}

/// A button that is on or off (a pane).
fn toggle_button(app: &RefAny, icon: &str, label: &str, which: Toggle, on: bool) -> RibbonButton {
    ribbon_kit::button(app, icon, label, Action::Toggle(which)).with_toggled(on)
}

/// A command AzDrive cannot do at all, greyed with the reason.
fn unavailable(icon: &str, text: &str, reason: &str) -> RibbonButton {
    RibbonButton::create(AzString::from(icon), label(text)).with_disabled(label(reason))
}

fn large(b: RibbonButton) -> RibbonItem {
    RibbonItem::LargeButton(b)
}

fn small(b: RibbonButton) -> RibbonItem {
    RibbonItem::SmallButton(b)
}

/// A check box with its label (Show/hide): both toggle `which`.
fn check(app: &RefAny, text: &str, which: Toggle, on: bool) -> RibbonItem {
    RibbonItem::Row(
        RibbonRow::create()
            .with_item(RibbonItem::Check(
                CheckBox::create(on)
                    .with_accessibility_name(label(text))
                    .with_on_toggle(
                        action_ref(app, Action::Toggle(which)),
                        on_check as CheckBoxOnToggleCallbackType,
                    ),
            ))
            .with_item(RibbonItem::Custom(
                Dom::create_span_with_text(label(text))
                    .with_css("font-size: 12px; margin-left: 4px; cursor: default;")
                    .with_callback(
                        EventFilter::Hover(HoverEventFilter::Click),
                        action_ref(app, Action::Toggle(which)),
                        on_action,
                    ),
            )),
    )
}

extern "C" fn on_check(mut data: RefAny, mut info: CallbackInfo, _state: CheckBoxState) -> Update {
    let Some((mut app, action)) = data
        .downcast_ref::<ActionRef>()
        .map(|r| (r.app.clone(), r.action.clone()))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| {
        actions::run_action(info, app, s, action)
    })
}

// ==== Home ====

fn home_tab(s: &DriveState, app: &RefAny) -> RibbonTab {
    let undo = s
        .undo
        .last()
        .map_or_else(|| String::from("azdrive-undo"), crate::UndoOp::label);
    RibbonTab::create(label(RibbonTabKind::Home.key()))
        .with_group(group(
            "azdrive-group-clipboard",
            vec![
                large(button(s, app, "content_copy", "azdrive-ribbon-copy", Action::Copy)),
                large(button(s, app, "content_paste", "azdrive-ribbon-paste", Action::Paste)),
                small(button(s, app, "content_cut", "azdrive-ribbon-cut", Action::Cut)),
                small(button(s, app, "link", "azdrive-ribbon-copy-path", Action::CopyPath)),
                small(unavailable(
                    "shortcut",
                    "azdrive-ribbon-paste-shortcut",
                    "azdrive-ribbon-paste-shortcut-why",
                )),
            ],
        ))
        .with_group(group(
            "azdrive-group-organize",
            vec![
                large(menu_button(
                    s,
                    app,
                    "drive_file_move",
                    "azdrive-ribbon-move-to",
                    Action::MoveToMenu,
                )),
                large(menu_button(
                    s,
                    app,
                    "file_copy",
                    "azdrive-ribbon-copy-to",
                    Action::CopyToMenu,
                )),
                large(split_button(
                    s,
                    app,
                    "delete",
                    "azdrive-ribbon-delete",
                    Action::Delete,
                    Action::DeleteMenu,
                )),
                large(button(
                    s,
                    app,
                    "drive_file_rename_outline",
                    "azdrive-ribbon-rename",
                    Action::Rename,
                )),
                small(button(s, app, "undo", &undo, Action::Undo)),
            ],
        ))
        .with_group(group(
            "azdrive-group-new",
            vec![
                large(button(
                    s,
                    app,
                    "create_new_folder",
                    "azdrive-ribbon-new-folder",
                    Action::NewFolder,
                )),
                small(menu_button(
                    s,
                    app,
                    "note_add",
                    "azdrive-ribbon-new-item",
                    Action::NewItemMenu,
                )),
                small(menu_button(
                    s,
                    app,
                    "bolt",
                    "azdrive-ribbon-easy-access",
                    Action::EasyAccessMenu,
                )),
                // Buy storage or connect a data source (Explorer's "Map network drive").
                small(button(s, app, "add_link", "azdrive-ribbon-add-drive", Action::AddDrive)),
            ],
        ))
        .with_group(group(
            "azdrive-ribbon-open",
            vec![
                large(split_button(
                    s,
                    app,
                    "info",
                    "azdrive-ribbon-properties",
                    Action::Properties,
                    Action::PropertiesMenu,
                )),
                small(split_button(
                    s,
                    app,
                    "open_in_new",
                    "azdrive-ribbon-open",
                    Action::Open,
                    Action::OpenMenu,
                )),
                small(button(s, app, "edit", "azdrive-ribbon-edit", Action::Edit)),
                small(unavailable(
                    "history",
                    "azdrive-ribbon-history",
"azdrive-ribbon-history-why",
                )),
            ],
        ))
        .with_group(group(
            "azdrive-group-select",
            vec![
                small(button(s, app, "select_all", "azdrive-ribbon-select-all", Action::SelectAll)),
                small(button(s, app, "deselect", "azdrive-ribbon-select-none", Action::SelectNone)),
                small(button(
                    s,
                    app,
                    "flip",
                    "azdrive-ribbon-invert-selection",
                    Action::InvertSelection,
                )),
            ],
        ))
}

// ==== Share ====

fn share_tab(s: &DriveState, app: &RefAny) -> RibbonTab {
    RibbonTab::create(label(RibbonTabKind::Share.key()))
        .with_group(group(
            "azdrive-group-send",
            vec![
                large(button(s, app, "email", "azdrive-ribbon-email", Action::Email)),
                large(button(s, app, "archive", "azdrive-ribbon-zip", Action::Zip)),
                small(button(s, app, "print", "azdrive-ribbon-print", Action::Print)),
                small(unavailable(
                    "album",
                    "azdrive-ribbon-burn",
                    "azdrive-ribbon-burn-why",
                )),
                small(unavailable("fax", "azdrive-ribbon-fax", "azdrive-ribbon-fax-why")),
            ],
        ))
        .with_group(group(
            "azdrive-group-cloud",
            vec![
                large(button(s, app, "upload", "azdrive-ribbon-upload", Action::Upload)),
                large(button(s, app, "download", "azdrive-ribbon-download", Action::Download)),
                large(button(s, app, "link", "azdrive-ribbon-copy-link", Action::Share)),
            ],
        ))
        .with_group(group(
            "azdrive-group-sync",
            vec![
                large(button(
                    s,
                    app,
                    "sync",
                    "azdrive-ribbon-sync-now",
                    Action::Sync(SyncAction::Now),
                )),
                small(button(
                    s,
                    app,
                    "push_pin",
                    "azdrive-ribbon-keep-on-device",
                    Action::Sync(SyncAction::KeepOnDevice),
                )),
                small(button(
                    s,
                    app,
                    "cloud_queue",
                    "azdrive-ribbon-free-up-space",
                    Action::Sync(SyncAction::FreeUpSpace),
                )),
                small(button(
                    s,
                    app,
                    "pause_circle",
                    if sync_paused(s) {
                        "azdrive-ribbon-resume-sync"
                    } else {
                        "azdrive-ribbon-pause-sync"
                    },
                    Action::Sync(SyncAction::Pause),
                )),
                small(button(
                    s,
                    app,
                    "drive_folder_upload",
                    "azdrive-ribbon-sync-with-folder",
                    Action::Sync(SyncAction::Pair),
                )),
            ],
        ))
        .with_group(group(
            "azdrive-group-share-with",
            vec![large(unavailable(
                "security",
                "azdrive-ribbon-advanced-security",
"azdrive-ribbon-advanced-security-why",
            ))],
        ))
}

/// Whether the drive the sync commands are about is paused.
fn sync_paused(s: &DriveState) -> bool {
    crate::sync_view::target_drive(s)
        .and_then(|id| crate::sync_view::setup_of(s, &id).map(|p| p.paused))
        .unwrap_or(false)
}

// ==== View ====

fn view_tab(s: &DriveState, app: &RefAny) -> RibbonTab {
    let cells: Vec<RibbonGalleryCell> = ViewLayout::ALL
        .iter()
        .map(|layout| {
            RibbonGalleryCell::create(
                Dom::create_icon(AzString::from(layout.icon())).with_css("font-size: 20px;"),
                label(layout.label()),
            )
        })
        .collect();
    let selected = ViewLayout::ALL
        .iter()
        .position(|l| *l == s.settings.layout)
        .unwrap_or(0);
    let hidden_selected = !s.selected_entries().is_empty()
        && s.selected_entries().iter().all(|e| e.is_hidden());
    let settings = &s.settings;
    RibbonTab::create(label(RibbonTabKind::View.key()))
        .with_group(group(
            "azdrive-group-panes",
            vec![
                large(toggle_button(
                    app,
                    "vertical_split",
                    "azdrive-ribbon-navigation-pane",
                    Toggle::NavigationPane,
                    settings.navigation_pane,
                )),
                small(toggle_button(
                    app,
                    "preview",
                    "azdrive-ribbon-preview-pane",
                    Toggle::PreviewPane,
                    settings.preview_pane,
                )),
                small(toggle_button(
                    app,
                    "view_sidebar",
                    "azdrive-ribbon-details-pane",
                    Toggle::DetailsPane,
                    settings.details_pane,
                )),
            ],
        ))
        .with_group(group(
            "azdrive-group-layout",
            vec![RibbonItem::Gallery(
                RibbonGallery::create(RibbonGalleryCellVec::from(cells))
                    .with_selected(selected)
                    .with_visible(4)
                    .with_on_select(
                        app.clone(),
                        on_layout_select as RibbonGalleryOnSelectCallbackType,
                    ),
            )],
        ))
        .with_group(group(
            "azdrive-group-current-view",
            vec![
                large(menu_button(s, app, "sort", "azdrive-ribbon-sort-by", Action::SortMenu)),
                small(menu_button(
                    s,
                    app,
                    "category",
                    "azdrive-ribbon-group-by",
                    Action::GroupMenu,
                )),
                small(menu_button(
                    s,
                    app,
                    "view_column",
                    "azdrive-ribbon-add-columns",
                    Action::ColumnsMenu,
                )),
                small(button(
                    s,
                    app,
                    "fit_screen",
                    "azdrive-ribbon-fit-columns",
                    Action::FitColumns,
                )),
            ],
        ))
        .with_group(group(
            "azdrive-group-show-hide",
            vec![
                check(
                    app,
                    "azdrive-ribbon-item-checkboxes",
                    Toggle::ItemCheckboxes,
                    settings.item_checkboxes,
                ),
                check(
                    app,
                    "azdrive-ribbon-extensions",
                    Toggle::Extensions,
                    settings.show_extensions,
                ),
                check(
                    app,
                    "azdrive-ribbon-hidden-items",
                    Toggle::HiddenItems,
                    settings.show_hidden,
                ),
                large(button(
                    s,
                    app,
                    if hidden_selected {
                        "visibility"
                    } else {
                        "visibility_off"
                    },
                    if hidden_selected {
                        "azdrive-ribbon-unhide-selected"
                    } else {
                        "azdrive-ribbon-hide-selected"
                    },
                    Action::HideSelected,
                )),
            ],
        ))
        .with_group(group(
            "azdrive-ribbon-options",
            vec![large(button(s, app, "tune", "azdrive-ribbon-options", Action::Options))],
        ))
}

extern "C" fn on_layout_select(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        if let Some(layout) = ViewLayout::ALL.get(index) {
            actions::set_layout(info, app, s, *layout);
        }
    })
}

// ==== Computer (This PC) ====

fn computer_tab(s: &DriveState, app: &RefAny) -> RibbonTab {
    RibbonTab::create(label(RibbonTabKind::Computer.key()))
        .with_group(group(
            "azdrive-group-location",
            vec![
                large(button(s, app, "info", "azdrive-ribbon-properties", Action::DriveProperties)),
                large(button(s, app, "open_in_new", "azdrive-ribbon-open", Action::Open)),
            ],
        ))
        .with_group(group(
            "azdrive-group-network",
            vec![
                large(button(s, app, "add_circle", "azdrive-ribbon-add-drive", Action::AddDrive)),
                large(button(
                    s,
                    app,
                    "create_new_folder",
                    "azdrive-ribbon-add-folder-drive",
                    Action::AddLocalDrive,
                )),
                large(button(
                    s,
                    app,
                    "remove_circle",
                    "azdrive-ribbon-remove-drive",
                    Action::RemoveDrive,
                )),
            ],
        ))
        .with_group(group(
            "azdrive-group-system",
            vec![
                large(button(s, app, "refresh", "azdrive-ribbon-refresh", Action::Refresh)),
                large(button(s, app, "tune", "azdrive-ribbon-options", Action::Options)),
            ],
        ))
}

// ==== Search (Search Tools) ====

/// Windows 8's Search tab while a search is open: whether the files' contents are read too
/// (greyed on a cloud drive, which is searched by name), hidden items, whether what .gitignore
/// files name is passed over - each change searches again -, and Close search.
fn search_tab(s: &DriveState, app: &RefAny) -> RibbonTab {
    let settings = &s.settings;
    let refines = &s.refines;
    let indexed = s
        .current_drive_id()
        .is_some_and(|id| settings.indexed_drives.contains(&id));
    // A refine that is set shows its choice on its button.
    let refine_label = |name: &str, chosen: Option<&str>| match chosen {
        Some(chosen) => t_args(
            "azdrive-refine-chosen",
            &[("name", t(name).into()), ("chosen", t_label(chosen).into())],
        ),
        None => name.to_string(),
    };
    let date = refine_label(
        "azdrive-ribbon-date-modified",
        (refines.date != crate::find::DateRefine::Any).then(|| refines.date.label()),
    );
    let kind = refine_label(
        "azdrive-ribbon-kind",
        (refines.kind != crate::find::KindRefine::Any).then(|| refines.kind.label()),
    );
    let size = refine_label(
        "azdrive-ribbon-size",
        (refines.size != crate::find::SizeRefine::Any).then(|| refines.size.label()),
    );
    RibbonTab::create(label(RibbonTabKind::Search.key()))
        .with_group(group(
            "azdrive-group-location",
            vec![
                large(
                    button(
                        s,
                        app,
                        "folder",
                        "azdrive-ribbon-current-folder",
                        Action::SearchSubfolders(false),
                    )
                        .with_toggled(!settings.search_subfolders),
                ),
                large(
                    button(
                        s,
                        app,
                        "account_tree",
                        "azdrive-ribbon-all-subfolders",
                        Action::SearchSubfolders(true),
                    )
                    .with_toggled(settings.search_subfolders),
                ),
            ],
        ))
        .with_group(group(
            "azdrive-group-refine",
            vec![
                large(menu_button(s, app, "event", &date, Action::RefineDateMenu)),
                small(menu_button(s, app, "category", &kind, Action::RefineKindMenu)),
                small(menu_button(s, app, "straighten", &size, Action::RefineSizeMenu)),
            ],
        ))
        .with_group(group(
            "azdrive-ribbon-options",
            vec![
                large(
                    button(
                        s,
                        app,
                        "find_in_page",
                        "azdrive-ribbon-file-contents",
                        Action::Toggle(Toggle::SearchContents),
                    )
                    .with_toggled(settings.search_contents),
                ),
                small(toggle_button(
                    app,
                    "visibility",
                    "azdrive-ribbon-hidden-items",
                    Toggle::HiddenItems,
                    settings.show_hidden,
                )),
                small(toggle_button(
                    app,
                    "filter_alt",
                    "azdrive-ribbon-skip-ignored",
                    Toggle::SearchIgnoreFiles,
                    settings.search_ignore_files,
                )),
                small(button(
                    s,
                    app,
                    "folder_open",
                    "azdrive-ribbon-open-file-location",
                    Action::OpenFileLocation,
                )),
                small(
                    button(
                        s,
                        app,
                        "manage_search",
                        "azdrive-ribbon-index-drive",
                        Action::IndexDrive,
                    )
                        .with_toggled(indexed),
                ),
                small(
                    button(
                        s,
                        app,
                        "cloud_download",
                        "azdrive-ribbon-index-cloud",
                        Action::Toggle(Toggle::IndexCloudFiles),
                    )
                    .with_toggled(settings.index_cloud_files),
                ),
            ],
        ))
        .with_group(group(
            "azdrive-group-saved",
            vec![
                large(button(
                    s,
                    app,
                    "bookmark_add",
                    "azdrive-ribbon-save-search",
                    Action::SaveSearch,
                )),
                small(menu_button(
                    s,
                    app,
                    "bookmarks",
                    "azdrive-ribbon-saved-searches",
                    Action::SavedSearchesMenu,
                )),
            ],
        ))
        .with_group(group(
            "azdrive-ribbon-close",
            vec![large(button(
                s,
                app,
                "search_off",
                "azdrive-ribbon-close-search",
                Action::CloseSearch,
            ))],
        ))
}

// ==== File: Windows 8's File menu ====

/// The File menu's commands, in Windows 8's order (their positions are the menu's events).
const NEW_WINDOW: usize = 0;
const TERMINAL: usize = 1;
const DELETE_HISTORY: usize = 2;
const HELP: usize = 3;
const CLOSE: usize = 4;

/// The places File > Frequent places lists: the pinned folders first, then the folders visited
/// last that are not pinned - ten at most, each with whether it is pinned.
pub(crate) fn frequent_places(s: &DriveState) -> Vec<(Place, String, bool)> {
    /// Places the list holds.
    const MAX: usize = 10;
    let mut places: Vec<(Place, String, bool)> = s
        .settings
        .pinned
        .iter()
        .map(|p| (Place::folder(&p.drive, &p.prefix), p.name.clone(), true))
        .collect();
    for place in &s.recent {
        if !matches!(place, Place::Folder { .. }) || places.iter().any(|(p, _, _)| p == place) {
            continue;
        }
        places.push((place.clone(), s.place_title(place), false));
    }
    places.truncate(MAX);
    places
}

/// File's menu: the commands on the left (a ▸ opens its sub-commands in the right column),
/// Frequent places on the right with their pins.
fn file_menu(s: &DriveState, app: &RefAny) -> Dom {
    let terminal_reason = why_not(s, &Action::OpenTerminal { azterm: false });
    let azterm_reason = why_not(s, &Action::OpenTerminal { azterm: true });
    let with_reason = |command: RibbonFileMenuCommand, reason: Option<String>| match reason {
        Some(reason) => command.with_disabled(label(&reason)),
        None => command,
    };
    let mut terminal = RibbonFileMenuCommand::create(
        AzString::from("terminal"),
        label("azdrive-file-terminal-here"),
    )
    .with_child(with_reason(
        RibbonFileMenuCommand::create(AzString::from("terminal"), label("azdrive-file-terminal"))
            .with_description(AzString::from(
                "azdrive-file-terminal-system",
            )),
        terminal_reason.clone(),
    ))
    .with_child(with_reason(
        RibbonFileMenuCommand::create(AzString::from("keyboard"), label("azdrive-file-azterm"))
            .with_description(AzString::from(
                "azdrive-file-azterm-about",
            )),
        azterm_reason,
    ));
    if let Some(reason) = terminal_reason {
        terminal = terminal.with_disabled(label(&reason));
    }
    let commands = vec![
        RibbonFileMenuCommand::create(
            AzString::from("open_in_new"),
            label("azdrive-file-new-window"),
        )
        .with_description(label("azdrive-file-new-window-about")),
        terminal,
        RibbonFileMenuCommand::create(
            AzString::from("history"),
            label("azdrive-file-delete-history"),
        )
            .with_separator_before(true)
            .with_child(
                RibbonFileMenuCommand::create(
                    AzString::from("schedule"),
                    label("azdrive-file-recent-places"),
                )
                .with_description(label("azdrive-file-recent-places-about")),
            )
            .with_child(
                RibbonFileMenuCommand::create(
                    AzString::from("swap_horiz"),
                    label("azdrive-file-back-forward"),
                )
                .with_description(label("azdrive-file-back-forward-about")),
            )
            .with_child(
                RibbonFileMenuCommand::create(
                    AzString::from("delete_sweep"),
                    label("azdrive-file-everything"),
                )
                .with_description(label("azdrive-file-everything-about")),
            ),
        RibbonFileMenuCommand::create(AzString::from("help"), label("azdrive-file-help"))
            .with_separator_before(true)
            .with_child(
                RibbonFileMenuCommand::create(
                    AzString::from("keyboard"),
                    label("azdrive-file-shortcuts"),
                )
                .with_description(label("azdrive-file-shortcuts-about")),
            )
            .with_child(
                RibbonFileMenuCommand::create(AzString::from("info"), label("azdrive-file-about"))
                    .with_description(label("azdrive-file-about-about")),
            ),
        RibbonFileMenuCommand::create(AzString::from("close"), label("azdrive-ribbon-close"))
            .with_separator_before(true),
    ];
    let places: Vec<RibbonFileMenuPlace> = frequent_places(s)
        .into_iter()
        .map(|(place, name, pinned)| {
            RibbonFileMenuPlace::create(AzString::from(name))
                .with_detail(AzString::from(browse::path_text(
                    &place,
                    Some(&s.drive_name(&place)),
                )))
                .with_pinned(pinned)
        })
        .collect();
    RibbonFileMenu::create(RibbonFileMenuCommandVec::from(commands))
        .with_places(
            label("azdrive-file-frequent-places"),
            RibbonFileMenuPlaceVec::from(places),
        )
        .with_on_event(app.clone(), on_file_menu as RibbonFileMenuOnEventCallbackType)
        .dom()
}

/// What a pick in the File menu runs (the menu calls this in the app's window once it closed;
/// a pin toggles while it stays open).
pub(crate) fn file_menu_action(s: &DriveState, event: &RibbonFileMenuEvent) -> Option<Action> {
    match event.kind {
        RibbonFileMenuEventKind::Command => match event.index {
            NEW_WINDOW => Some(Action::NewWindow),
            CLOSE => Some(Action::CloseWindow),
            _ => None,
        },
        RibbonFileMenuEventKind::SubCommand => match (event.index, event.sub_index) {
            (TERMINAL, 0) => Some(Action::OpenTerminal { azterm: false }),
            (TERMINAL, 1) => Some(Action::OpenTerminal { azterm: true }),
            (DELETE_HISTORY, 0) => Some(Action::ClearHistory {
                recent: true,
                back_forward: false,
            }),
            (DELETE_HISTORY, 1) => Some(Action::ClearHistory {
                recent: false,
                back_forward: true,
            }),
            (DELETE_HISTORY, 2) => Some(Action::ClearHistory {
                recent: true,
                back_forward: true,
            }),
            (HELP, 0) => Some(Action::Shortcuts),
            (HELP, 1) => Some(Action::About),
            _ => None,
        },
        RibbonFileMenuEventKind::OpenPlace => frequent_places(s)
            .into_iter()
            .nth(event.index)
            .map(|(place, _, _)| Action::Go(place)),
        RibbonFileMenuEventKind::TogglePin => frequent_places(s)
            .into_iter()
            .nth(event.index)
            .map(|(place, _, _)| Action::TogglePin(place)),
    }
}

extern "C" fn on_file_menu(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: RibbonFileMenuEvent,
) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        if let Some(action) = file_menu_action(s, &event) {
            println!("AZDRIVE_FILE_MENU {action:?}");
            actions::run_action(info, app, s, action);
        }
    })
}

// ==== The ribbon ====

/// The ribbon for the open place; the tab strip is the window's title bar.
pub(crate) fn ribbon(s: &DriveState, app: &RefAny) -> Dom {
    let searching = s.find.is_some();
    let tabs: Vec<RibbonTab> = tabs_of(&s.place, searching)
        .iter()
        .map(|tab| match tab {
            RibbonTabKind::Home => home_tab(s, app),
            RibbonTabKind::Share => share_tab(s, app),
            RibbonTabKind::View => view_tab(s, app),
            RibbonTabKind::Computer => computer_tab(s, app),
            RibbonTabKind::Search => search_tab(s, app),
        })
        .collect();
    Ribbon::create(tabs)
        .with_app_button(
            RibbonAppButton::create(label("azdrive-tab-file")).with_menu(file_menu(s, app)),
        )
        .with_active_tab(active_index(&s.place, searching, s.ribbon_tab))
        .with_on_tab_click(app.clone(), on_ribbon_tab as RibbonOnTabClickCallbackType)
        // No title row over the ribbon: its tabs are the title bar.
        .with_tabs_in_titlebar(azul_appkit::ui::tabs_in_titlebar())
        .dom_desktop()
        .with_id(ids::RIBBON)
}

extern "C" fn on_ribbon_tab(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    let Some(mut s) = data.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    let Some(tab) = tabs_of(&s.place, s.find.is_some()).get(index).copied() else {
        return Update::DoNothing;
    };
    s.ribbon_tab = tab;
    println!("AZDRIVE_RIBBON_TAB {}", tab.label());
    Update::RefreshDom
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder shows Home, Share and View; This PC Computer and View - as Windows 8's Explorer.
    #[test]
    fn a_folder_shows_home_share_view_and_this_pc_shows_computer_and_view() {
        let folder = Place::folder("home", "Documents/");
        assert_eq!(
            tabs_of(&folder, false),
            &[RibbonTabKind::Home, RibbonTabKind::Share, RibbonTabKind::View]
        );
        assert_eq!(
            tabs_of(&Place::ThisPc, false),
            &[RibbonTabKind::Computer, RibbonTabKind::View]
        );
        assert_eq!(tabs_of(&Place::QuickAccess, false)[0], RibbonTabKind::Home);
    }

    /// The tab chosen last stays where the place has it (View from a folder to This PC) and
    /// falls back to the place's first tab where it has not (Share at This PC).
    #[test]
    fn the_chosen_tab_stays_where_the_place_has_it() {
        let folder = Place::folder("home", "");
        assert_eq!(active_index(&folder, false, RibbonTabKind::View), 2);
        assert_eq!(active_index(&Place::ThisPc, false, RibbonTabKind::View), 1);
        assert_eq!(active_index(&Place::ThisPc, false, RibbonTabKind::Share), 0);
        assert_eq!(active_index(&folder, false, RibbonTabKind::Computer), 0);
    }

    /// While a search of a folder is open, Windows 8's Search tab (Search Tools) follows View;
    /// it goes with the search, and the place's first tab shows again.
    #[test]
    fn a_search_adds_the_search_tab_after_view() {
        let folder = Place::folder("home", "Documents/");
        assert_eq!(
            tabs_of(&folder, true),
            &[
                RibbonTabKind::Home,
                RibbonTabKind::Share,
                RibbonTabKind::View,
                RibbonTabKind::Search
            ]
        );
        assert_eq!(active_index(&folder, true, RibbonTabKind::Search), 3);
        assert_eq!(active_index(&folder, false, RibbonTabKind::Search), 0);
        assert_eq!(RibbonTabKind::Search.label(), "Search");
        assert_eq!(
            tabs_of(&Place::ThisPc, true),
            &[RibbonTabKind::Computer, RibbonTabKind::View, RibbonTabKind::Search],
            "This PC searches every drive on this computer"
        );
        assert_eq!(active_index(&Place::ThisPc, true, RibbonTabKind::Search), 2);
    }
}
