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
//! Search (Search Tools, while a search is open): Options [File contents] Hidden items /
//!        Skip ignored files  Close [Close search]
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
use azul_appkit::ribbon::{self as ribbon_kit, group, RibbonCommand};

use crate::{
    actions::{self, action_ref, on_action, why_not, Action, ActionRef, Toggle},
    browse::{self, Place},
    ids,
    model::ViewLayout,
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
    /// This PC's tab (Windows 8's "Computer"): the drives.
    Computer,
    /// Windows 8's Search tab (Search Tools), while a search of a folder is open.
    Search,
}

impl RibbonTabKind {
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
fn button(s: &DriveState, app: &RefAny, icon: &str, label: &str, action: Action) -> RibbonButton {
    let reason = why_not(s, &action);
    let b = ribbon_kit::button(app, icon, label, action);
    match reason {
        Some(reason) => b.with_disabled(AzString::from(reason)),
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
fn unavailable(icon: &str, label: &str, reason: &str) -> RibbonButton {
    RibbonButton::create(AzString::from(icon), AzString::from(label))
        .with_disabled(AzString::from(reason))
}

fn large(b: RibbonButton) -> RibbonItem {
    RibbonItem::LargeButton(b)
}

fn small(b: RibbonButton) -> RibbonItem {
    RibbonItem::SmallButton(b)
}

/// A check box with its label (Show/hide): both toggle `which`.
fn check(app: &RefAny, label: &str, which: Toggle, on: bool) -> RibbonItem {
    RibbonItem::Row(
        RibbonRow::create()
            .with_item(RibbonItem::Check(
                CheckBox::create(on)
                    .with_accessibility_name(AzString::from(label))
                    .with_on_toggle(
                        action_ref(app, Action::Toggle(which)),
                        on_check as CheckBoxOnToggleCallbackType,
                    ),
            ))
            .with_item(RibbonItem::Custom(
                Dom::create_span_with_text(AzString::from(label))
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
        .map_or_else(|| String::from("Undo"), crate::UndoOp::label);
    RibbonTab::create(AzString::from(RibbonTabKind::Home.label()))
        .with_group(group(
            "Clipboard",
            vec![
                large(button(s, app, "content_copy", "Copy", Action::Copy)),
                large(button(s, app, "content_paste", "Paste", Action::Paste)),
                small(button(s, app, "content_cut", "Cut", Action::Cut)),
                small(button(s, app, "link", "Copy path", Action::CopyPath)),
                small(unavailable(
                    "shortcut",
                    "Paste shortcut",
                    "A shortcut is a Windows link file; a drive here holds files and folders only.",
                )),
            ],
        ))
        .with_group(group(
            "Organize",
            vec![
                large(menu_button(s, app, "drive_file_move", "Move to", Action::MoveToMenu)),
                large(menu_button(s, app, "file_copy", "Copy to", Action::CopyToMenu)),
                large(split_button(
                    s,
                    app,
                    "delete",
                    "Delete",
                    Action::Delete,
                    Action::DeleteMenu,
                )),
                large(button(
                    s,
                    app,
                    "drive_file_rename_outline",
                    "Rename",
                    Action::Rename,
                )),
                small(button(s, app, "undo", &undo, Action::Undo)),
            ],
        ))
        .with_group(group(
            "New",
            vec![
                large(button(s, app, "create_new_folder", "New folder", Action::NewFolder)),
                small(menu_button(s, app, "note_add", "New item", Action::NewItemMenu)),
                small(menu_button(s, app, "bolt", "Easy access", Action::EasyAccessMenu)),
                // Buy storage or connect a data source (Explorer's "Map network drive").
                small(button(s, app, "add_link", "Add drive", Action::AddDrive)),
            ],
        ))
        .with_group(group(
            "Open",
            vec![
                large(split_button(
                    s,
                    app,
                    "info",
                    "Properties",
                    Action::Properties,
                    Action::PropertiesMenu,
                )),
                small(split_button(s, app, "open_in_new", "Open", Action::Open, Action::OpenMenu)),
                small(button(s, app, "edit", "Edit", Action::Edit)),
                small(unavailable(
                    "history",
                    "History",
                    "Earlier versions need a versioned bucket; AzDrive keeps one version of a \
                     file.",
                )),
            ],
        ))
        .with_group(group(
            "Select",
            vec![
                small(button(s, app, "select_all", "Select all", Action::SelectAll)),
                small(button(s, app, "deselect", "Select none", Action::SelectNone)),
                small(button(s, app, "flip", "Invert selection", Action::InvertSelection)),
            ],
        ))
}

// ==== Share ====

fn share_tab(s: &DriveState, app: &RefAny) -> RibbonTab {
    RibbonTab::create(AzString::from(RibbonTabKind::Share.label()))
        .with_group(group(
            "Send",
            vec![
                large(button(s, app, "email", "Email", Action::Email)),
                large(button(s, app, "archive", "Zip", Action::Zip)),
                small(button(s, app, "print", "Print", Action::Print)),
                small(unavailable(
                    "album",
                    "Burn to disc",
                    "AzDrive cannot write discs: this computer has no burner it can use.",
                )),
                small(unavailable("fax", "Fax", "No fax device is set up.")),
            ],
        ))
        .with_group(group(
            "Cloud",
            vec![
                large(button(s, app, "upload", "Upload", Action::Upload)),
                large(button(s, app, "download", "Download", Action::Download)),
                large(button(s, app, "link", "Copy link", Action::Share)),
            ],
        ))
        .with_group(group(
            "Share with",
            vec![large(unavailable(
                "security",
                "Advanced security",
                "Who may read a cloud drive is set in its bucket policy; a local folder's in the \
                 system's sharing settings.",
            ))],
        ))
}

// ==== View ====

fn view_tab(s: &DriveState, app: &RefAny) -> RibbonTab {
    let cells: Vec<RibbonGalleryCell> = ViewLayout::ALL
        .iter()
        .map(|layout| {
            RibbonGalleryCell::create(
                Dom::create_icon(AzString::from(layout.icon())).with_css("font-size: 20px;"),
                AzString::from(layout.label()),
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
    RibbonTab::create(AzString::from(RibbonTabKind::View.label()))
        .with_group(group(
            "Panes",
            vec![
                large(toggle_button(
                    app,
                    "vertical_split",
                    "Navigation pane",
                    Toggle::NavigationPane,
                    settings.navigation_pane,
                )),
                small(toggle_button(
                    app,
                    "preview",
                    "Preview pane",
                    Toggle::PreviewPane,
                    settings.preview_pane,
                )),
                small(toggle_button(
                    app,
                    "view_sidebar",
                    "Details pane",
                    Toggle::DetailsPane,
                    settings.details_pane,
                )),
            ],
        ))
        .with_group(group(
            "Layout",
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
            "Current view",
            vec![
                large(menu_button(s, app, "sort", "Sort by", Action::SortMenu)),
                small(menu_button(s, app, "category", "Group by", Action::GroupMenu)),
                small(menu_button(s, app, "view_column", "Add columns", Action::ColumnsMenu)),
                small(button(
                    s,
                    app,
                    "fit_screen",
                    "Size all columns to fit",
                    Action::FitColumns,
                )),
            ],
        ))
        .with_group(group(
            "Show/hide",
            vec![
                check(
                    app,
                    "Item check boxes",
                    Toggle::ItemCheckboxes,
                    settings.item_checkboxes,
                ),
                check(
                    app,
                    "File name extensions",
                    Toggle::Extensions,
                    settings.show_extensions,
                ),
                check(app, "Hidden items", Toggle::HiddenItems, settings.show_hidden),
                large(button(
                    s,
                    app,
                    if hidden_selected {
                        "visibility"
                    } else {
                        "visibility_off"
                    },
                    if hidden_selected {
                        "Unhide selected items"
                    } else {
                        "Hide selected items"
                    },
                    Action::HideSelected,
                )),
            ],
        ))
        .with_group(group(
            "Options",
            vec![large(button(s, app, "tune", "Options", Action::Options))],
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
    RibbonTab::create(AzString::from(RibbonTabKind::Computer.label()))
        .with_group(group(
            "Location",
            vec![
                large(button(s, app, "info", "Properties", Action::DriveProperties)),
                large(button(s, app, "open_in_new", "Open", Action::Open)),
            ],
        ))
        .with_group(group(
            "Network",
            vec![
                large(button(s, app, "add_circle", "Add drive", Action::AddDrive)),
                large(button(
                    s,
                    app,
                    "create_new_folder",
                    "Add folder as drive",
                    Action::AddLocalDrive,
                )),
                large(button(s, app, "remove_circle", "Remove drive", Action::RemoveDrive)),
            ],
        ))
        .with_group(group(
            "System",
            vec![
                large(button(s, app, "refresh", "Refresh", Action::Refresh)),
                large(button(s, app, "tune", "Options", Action::Options)),
            ],
        ))
}

// ==== Search (Search Tools) ====

/// Windows 8's Search tab while a search is open: whether the files' contents are read too
/// (greyed on a cloud drive, which is searched by name), hidden items, whether what .gitignore
/// files name is passed over - each change searches again -, and Close search.
fn search_tab(s: &DriveState, app: &RefAny) -> RibbonTab {
    let settings = &s.settings;
    RibbonTab::create(AzString::from(RibbonTabKind::Search.label()))
        .with_group(group(
            "Options",
            vec![
                large(
                    button(
                        s,
                        app,
                        "find_in_page",
                        "File contents",
                        Action::Toggle(Toggle::SearchContents),
                    )
                    .with_toggled(settings.search_contents),
                ),
                small(toggle_button(
                    app,
                    "visibility",
                    "Hidden items",
                    Toggle::HiddenItems,
                    settings.show_hidden,
                )),
                small(toggle_button(
                    app,
                    "filter_alt",
                    "Skip ignored files",
                    Toggle::SearchIgnoreFiles,
                    settings.search_ignore_files,
                )),
            ],
        ))
        .with_group(group(
            "Close",
            vec![large(button(s, app, "search_off", "Close search", Action::CloseSearch))],
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
        Some(reason) => command.with_disabled(AzString::from(reason)),
        None => command,
    };
    let mut terminal = RibbonFileMenuCommand::create(
        AzString::from("terminal"),
        AzString::from("Open terminal here"),
    )
    .with_child(with_reason(
        RibbonFileMenuCommand::create(AzString::from("terminal"), AzString::from("Terminal"))
            .with_description(AzString::from(
                "The system's terminal, in the open folder.",
            )),
        terminal_reason.clone(),
    ))
    .with_child(with_reason(
        RibbonFileMenuCommand::create(AzString::from("keyboard"), AzString::from("AzTerm"))
            .with_description(AzString::from(
                "Azlin's terminal, in the open folder.",
            )),
        azterm_reason,
    ));
    if let Some(reason) = terminal_reason {
        terminal = terminal.with_disabled(AzString::from(reason));
    }
    let commands = vec![
        RibbonFileMenuCommand::create(
            AzString::from("open_in_new"),
            AzString::from("Open new window"),
        )
        .with_description(AzString::from("Another AzDrive window, at this place.")),
        terminal,
        RibbonFileMenuCommand::create(AzString::from("history"), AzString::from("Delete history"))
            .with_separator_before(true)
            .with_child(
                RibbonFileMenuCommand::create(
                    AzString::from("schedule"),
                    AzString::from("Recent places"),
                )
                .with_description(AzString::from(
                    "Forget the places Recent locations and Frequent places remember (the pins \
                     stay).",
                )),
            )
            .with_child(
                RibbonFileMenuCommand::create(
                    AzString::from("swap_horiz"),
                    AzString::from("Back and forward"),
                )
                .with_description(AzString::from("Forget where Back and Forward go.")),
            )
            .with_child(
                RibbonFileMenuCommand::create(
                    AzString::from("delete_sweep"),
                    AzString::from("Everything"),
                )
                .with_description(AzString::from("Both.")),
            ),
        RibbonFileMenuCommand::create(AzString::from("help"), AzString::from("Help"))
            .with_separator_before(true)
            .with_child(
                RibbonFileMenuCommand::create(
                    AzString::from("keyboard"),
                    AzString::from("Keyboard shortcuts"),
                )
                .with_description(AzString::from("Every key AzDrive knows (F1).")),
            )
            .with_child(
                RibbonFileMenuCommand::create(AzString::from("info"), AzString::from("About AzDrive"))
                    .with_description(AzString::from("The version, the license, the credits.")),
            ),
        RibbonFileMenuCommand::create(AzString::from("close"), AzString::from("Close"))
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
            AzString::from("Frequent places"),
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
        .with_app_button(RibbonAppButton::create(AzString::from("File")).with_menu(file_menu(s, app)))
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
            &[RibbonTabKind::Computer, RibbonTabKind::View],
            "This PC is not searched"
        );
    }
}
