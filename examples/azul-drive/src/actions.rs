//! What AzDrive does: every command of the ribbon, its File menu and drop-downs,
//! the context menus and the keyboard is an [`Action`], run by [`run_action`] on
//! the state. The
//! storage work goes to a `Thread` ([`crate::spawn`]); this module decides
//! what to ask for, keeps the transfer queue moving, and answers the
//! threads' results that need more than a refresh.

use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
    time::{SystemTime, UNIX_EPOCH},
};

use azul::{
    dialog::{FileDialog, FileOpenMultiResult, FileOpenResult},
    dom::{ClipboardContent, DomNodeId, FocusTarget, VirtualKeyCode},
    menu::{Menu, MenuItem, MenuItemIcon, MenuItemState, MenuPopupPosition, StringMenuItem},
    option::{OptionFileTypeList, OptionMenuItemIcon},
    prelude::*,
    str::String as AzString,
    url::Url,
    vec::StyledTextRunVec,
};
use azul_storage::{
    azul_transport::AzulTransport,
    config::{self, DriveEntry, DriveLocation, DrivesFile},
    key, Drive, DriveError, LocalDrive, S3Drive,
};

use crate::{
    browse::{self, Column, DriveForm, Place},
    fileops::{self, ConflictChoice, Plan, SourceItem, TransferKind, TransferReport},
    go,
    jobs::{Job, PreviewContent},
    keys::{self, Command, Key, Mods, Step},
    listing,
    model::{self, GroupBy, ViewLayout},
    open_current, open_drive, place_up, preview, refresh, save_settings, spawn, ui_view,
    with_state, ClipboardItems, DriveState, KeyringCall, KeyringOp, Popup, PreviewState,
    PropertiesState, Renaming, Slot, TransferJob, UndoOp,
};

// ==== Actions ====

/// A setting the ribbon's View tab turns on and off.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Toggle {
    NavigationPane,
    PreviewPane,
    DetailsPane,
    ItemCheckboxes,
    Extensions,
    HiddenItems,
    ConfirmDelete,
}

/// Every command of the ribbon, its File menu, the menus and the backstage.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Action {
    // Clipboard
    Pin,
    Copy,
    Cut,
    Paste,
    CopyPath,
    // Organize
    MoveToMenu,
    CopyToMenu,
    MoveTo(Place),
    CopyTo(Place),
    ChooseLocation(TransferKind),
    DeleteMenu,
    Delete,
    DeletePermanently,
    Rename,
    // New
    NewFolder,
    NewItemMenu,
    NewTextDocument,
    NewEmptyFile,
    // Open
    Properties,
    Open,
    OpenMenu,
    Edit,
    // Select
    SelectAll,
    SelectNone,
    InvertSelection,
    // Share
    Share,
    Email,
    Zip,
    Download,
    Upload,
    // View
    Toggle(Toggle),
    SetLayout(ViewLayout),
    SortMenu,
    SortBy(Column),
    SortDescending(bool),
    GroupMenu,
    GroupBy(GroupBy),
    ColumnsMenu,
    ToggleColumn(Column),
    FitColumns,
    Options,
    /// The backstage's About page.
    About,
    // Drives
    AddDrive,
    AddLocalDrive,
    RemoveDrive,
    DriveProperties,
    Refresh,
    // Misc
    Undo,
    Go(Place),
    RecentMenu,
    CloseBackstage,
    CloseWindow,
    ShowTransfers,
    // File (Windows 8's File menu)
    /// Another AzDrive window - a process of its own - at the open place.
    NewWindow,
    /// A terminal in the open folder: the system's, or Azlin's AzTerm.
    OpenTerminal {
        azterm: bool,
    },
    /// Forget the places visited (Recent locations, Frequent places) and / or where Back and
    /// Forward go.
    ClearHistory {
        recent: bool,
        back_forward: bool,
    },
    /// The Options at the keyboard shortcuts (File > Help).
    Shortcuts,
    /// Pin a folder to Quick access, or unpin it (File > Frequent places' pins).
    TogglePin(Place),
    // Home
    /// New > Easy access: pin the folder, add a folder as a drive.
    EasyAccessMenu,
    /// Open > Properties' arrow: the item's or the drive's properties.
    PropertiesMenu,
    // Share
    /// The selected files to this computer's printer.
    Print,
    // View
    /// Hide the selected items (a name with a leading dot is hidden), or show them again when
    /// they all are hidden.
    HideSelected,
}

/// A button's / menu item's click data.
pub(crate) struct ActionRef {
    pub app: RefAny,
    pub action: Action,
}

/// The data a button or menu item carries for `action`.
pub(crate) fn action_ref(app: &RefAny, action: Action) -> RefAny {
    RefAny::new(ActionRef {
        app: app.clone(),
        action,
    })
}

/// A button, a menu item or a backstage button was clicked.
pub(crate) extern "C" fn on_action(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, action)) = data
        .downcast_ref::<ActionRef>()
        .map(|r| (r.app.clone(), r.action.clone()))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| {
        run_action(info, app, s, action)
    })
}

/// A menu entry running `action`; `disabled` greys it.
pub(crate) fn menu_item(app: &RefAny, label: &str, action: Action, disabled: bool) -> MenuItem {
    let mut item =
        StringMenuItem::create(AzString::from(label)).with_callback(action_ref(app, action), on_action);
    if disabled {
        item.menu_item_state = MenuItemState::Greyed;
    }
    MenuItem::String(item)
}

/// A menu entry with a check mark.
pub(crate) fn check_item(app: &RefAny, label: &str, action: Action, checked: bool) -> MenuItem {
    let mut item =
        StringMenuItem::create(AzString::from(label)).with_callback(action_ref(app, action), on_action);
    item.icon = OptionMenuItemIcon::Some(MenuItemIcon::Checkbox(checked));
    MenuItem::String(item)
}

/// A menu entry running `action`, greyed when it cannot run now.
fn able_item(app: &RefAny, s: &DriveState, label: &str, action: Action) -> MenuItem {
    let disabled = why_not(s, &action).is_some();
    menu_item(app, label, action, disabled)
}

/// Opens `items` as a drop-down under the button that asked for it (a ribbon button's arrow,
/// the address bar's chevron); where the pointer is when there is no button.
pub(crate) fn open_menu_below(info: &mut CallbackInfo, items: Vec<MenuItem>) {
    let menu = Menu::create(items).with_popup_position(MenuPopupPosition::BottomOfHitRect);
    if !info.open_menu_for_hit_node(menu.clone()) {
        info.open_menu(menu);
    }
}

/// Why `action` cannot run now, or `None` when it can (the ribbon greys the
/// control and says why).
pub(crate) fn why_not(s: &DriveState, action: &Action) -> Option<String> {
    let in_folder = s.current_drive().is_some();
    let selected = !s.selection.is_empty() && in_folder;
    let need_folder = || {
        (!in_folder).then(|| String::from("Open a folder of a drive first."))
    };
    let need_selection = || {
        if !in_folder {
            Some(String::from("Open a folder and select items first."))
        } else if s.selection.is_empty() {
            Some(String::from("Select one or more items first."))
        } else {
            None
        }
    };
    match action {
        Action::Pin => match &s.place {
            Place::Folder { .. } => None,
            _ => Some(String::from("Open a folder to pin it to Quick access.")),
        },
        Action::Copy | Action::Cut | Action::CopyPath | Action::MoveToMenu | Action::CopyToMenu
        | Action::DeleteMenu | Action::Delete | Action::DeletePermanently | Action::Zip
        | Action::Share | Action::Email => need_selection(),
        Action::Paste => need_folder().or_else(|| {
            s.clipboard
                .is_none()
                .then(|| String::from("Nothing to paste: copy or cut something first."))
        }),
        Action::Rename => need_selection().or_else(|| {
            (s.selection.len() != 1).then(|| String::from("Select exactly one item to rename."))
        }),
        Action::NewFolder | Action::NewItemMenu | Action::NewTextDocument
        | Action::NewEmptyFile | Action::Upload | Action::SelectAll | Action::InvertSelection => {
            need_folder()
        }
        Action::SelectNone => need_selection(),
        Action::Download => need_selection(),
        Action::Open | Action::OpenMenu => match &s.place {
            Place::ThisPc if s.selected_drive.is_none() => {
                Some(String::from("Select a drive to open."))
            }
            Place::QuickAccess if s.selected_pin.is_none() => {
                Some(String::from("Select a pinned folder to open."))
            }
            Place::Folder { .. } if !selected => Some(String::from("Select an item to open.")),
            _ => None,
        },
        Action::Edit => need_selection().or_else(|| {
            match s.single_selected() {
                Some(e) if !e.is_folder => None,
                Some(_) => Some(String::from("Edit opens a file; this is a folder.")),
                None => Some(String::from("Select one file to edit.")),
            }
        }),
        Action::RemoveDrive => match s.selected_drive.or(s.current_drive()) {
            Some(i) if s.slots.get(i).is_some_and(Slot::is_built_in) => {
                Some(String::from("Home and the Azlin data folder stay."))
            }
            Some(_) => None,
            None => Some(String::from("Select a drive on This PC first.")),
        },
        Action::DriveProperties => (s.selected_drive.or(s.current_drive()).is_none())
            .then(|| String::from("Select a drive on This PC first.")),
        Action::Undo => s
            .undo
            .is_empty()
            .then(|| String::from("Nothing to undo.")),
        Action::SortMenu | Action::GroupMenu | Action::ColumnsMenu | Action::FitColumns => {
            need_folder()
        }
        Action::OpenTerminal { azterm } => match s.current_drive() {
            Some(i) if s.slots[i].is_local() => (*azterm && sibling_app("AzTerm").is_none())
                .then(|| String::from("AzTerm is not installed next to AzDrive.")),
            Some(_) => Some(String::from(
                "A terminal opens in a folder of this computer; this folder is in an S3 bucket.",
            )),
            None => Some(String::from(
                "Open a folder of a drive on this computer first.",
            )),
        },
        Action::Print => need_selection().or_else(|| {
            if !s.current_drive().is_some_and(|i| s.slots[i].is_local()) {
                Some(String::from(
                    "Printing goes through this computer: download the file first.",
                ))
            } else if s.selected_entries().iter().any(|e| e.is_folder) {
                Some(String::from(
                    "Select files to print; a folder does not print.",
                ))
            } else {
                None
            }
        }),
        Action::HideSelected => need_selection(),
        _ => None,
    }
}

/// Runs `action` (or says why it cannot run).
pub(crate) fn run_action(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, action: Action) {
    if let Some(reason) = why_not(s, &action) {
        s.warn(reason);
        return;
    }
    match action {
        Action::Pin => toggle_pin(info, app, s),
        Action::Copy => copy_selected(s, false),
        Action::Cut => copy_selected(s, true),
        Action::Paste => paste(info, app, s),
        Action::CopyPath => copy_path(info, s),
        Action::MoveToMenu => destination_menu(info, app, s, TransferKind::Move),
        Action::CopyToMenu => destination_menu(info, app, s, TransferKind::Copy),
        Action::MoveTo(place) => transfer_selection_to(info, app, s, TransferKind::Move, place),
        Action::CopyTo(place) => transfer_selection_to(info, app, s, TransferKind::Copy, place),
        Action::ChooseLocation(kind) => {
            // The system's folder dialog, at the open folder when it is on this computer.
            let start = s
                .current_drive()
                .and_then(|i| s.local_dir(i, &s.prefix().to_string()))
                .map(|dir| AzString::from(dir.to_string_lossy().into_owned()));
            let _request = FileDialog::open_directory(
                AzString::from(match kind {
                    TransferKind::Move => "Move the items to",
                    _ => "Copy the items to",
                }),
                match start {
                    Some(dir) => OptionString::Some(dir),
                    None => OptionString::None,
                },
                RefAny::new(TransferPick {
                    app: app.clone(),
                    kind,
                }),
                on_destination_picked,
            );
        }
        Action::DeleteMenu => open_menu_below(info, delete_items(app, s)),
        Action::Delete => delete_selected(info, app, s, false),
        Action::DeletePermanently => delete_selected(info, app, s, true),
        Action::Rename => start_rename(s),
        Action::NewFolder => new_item(info, app, s, "New folder", true),
        Action::NewItemMenu => open_menu_below(info, new_items(app)),
        Action::NewTextDocument => new_item(info, app, s, "New Text Document.txt", false),
        Action::NewEmptyFile => new_item(info, app, s, "New file", false),
        Action::Properties => show_properties(info, app, s),
        Action::Open | Action::Edit => open_selected(info, app, s),
        Action::OpenMenu => open_menu_below(info, open_items(app)),
        Action::SelectAll => select_all(info, app, s),
        Action::SelectNone => {
            s.selection.clear();
            s.print_selection();
        }
        Action::InvertSelection => {
            let keys = s.visible_keys();
            let order: Vec<&str> = keys.iter().map(String::as_str).collect();
            s.selection.invert(&order);
            s.print_selection();
        }
        Action::Share => share_link(info, s),
        Action::Email => email_selected(s),
        Action::Zip => zip_selected(info, app, s),
        Action::Download => download_selected(info, app, s),
        Action::Upload => {
            let _request = FileDialog::open_multiple_files(
                AzString::from("Upload files"),
                OptionString::None,
                OptionFileTypeList::None,
                app.clone(),
                on_upload_picked,
            );
        }
        Action::Toggle(which) => toggle(info, app, s, which),
        Action::SetLayout(layout) => set_layout(info, app, s, layout),
        Action::SortMenu => open_menu_below(info, sort_items(app, s)),
        Action::SortBy(column) => sort_by(info, app, s, column, None),
        Action::SortDescending(descending) => {
            let column = s.settings.sort.column;
            sort_by(info, app, s, column, Some(descending));
        }
        Action::GroupMenu => open_menu_below(info, group_items(app, s)),
        Action::GroupBy(group) => {
            s.settings.group_by = group;
            println!("AZDRIVE_GROUP {}", group.label());
            // Groups by size or date need every item's stat (the scan reads names only).
            if needs_all_stats(s) {
                request_sort_stats(info, app, s);
            }
            request_view_work(info, app, s);
            save_settings(info, app, s);
        }
        Action::ColumnsMenu => open_menu_below(info, column_items(app, s)),
        Action::ToggleColumn(column) => {
            s.settings.columns.toggle(column);
            save_settings(info, app, s);
        }
        Action::FitColumns => {
            let show_extensions = s.settings.show_extensions;
            let entries: Vec<browse::Entry> = s.visible_entries().into_iter().cloned().collect();
            let refs: Vec<&browse::Entry> = entries.iter().collect();
            s.settings.columns.fit(&refs, show_extensions);
            save_settings(info, app, s);
        }
        Action::Options => {
            let was_open = azul_appkit::ui::settings_open(&s.kit);
            s.backstage = Some(0);
            azul_appkit::ui::open_settings(&s.kit, Some("View"));
            crate::options_opened(s, was_open);
        }
        Action::About => s.backstage = Some(1),
        Action::AddDrive => {
            if s.popup.is_none() {
                open_drive_form(s, None);
            }
        }
        Action::AddLocalDrive => {
            let _request = FileDialog::open_directory(
                AzString::from("Add a folder as a drive"),
                OptionString::None,
                app.clone(),
                on_local_drive_picked,
            );
        }
        Action::RemoveDrive => {
            if let Some(index) = s.selected_drive.or(s.current_drive()) {
                let drive_id = s.slots[index].entry.id.clone();
                s.popups_opened += 1;
                s.popup = Some(Popup::ConfirmForget { drive_id });
            }
        }
        Action::DriveProperties => {
            if let Some(index) = s.selected_drive.or(s.current_drive()) {
                open_properties(info, app, s, Vec::new(), Some(index));
            }
        }
        Action::Refresh => refresh(info, app, s),
        Action::Undo => undo(info, app, s),
        Action::Go(place) => go(info, app, s, place, true),
        Action::RecentMenu => {
            let items: Vec<MenuItem> = s
                .recent
                .iter()
                .map(|place| {
                    let label = browse::path_text(place, Some(&s.drive_name(place)));
                    menu_item(app, &label, Action::Go(place.clone()), *place == s.place)
                })
                .collect();
            if items.is_empty() {
                s.info("No places visited yet.");
            } else {
                open_menu_below(info, items);
            }
        }
        Action::CloseBackstage => s.backstage = None,
        Action::CloseWindow => info.close_window(),
        Action::ShowTransfers => {
            s.popups_opened += 1;
            s.popup = Some(Popup::Transfers { auto: false });
        }
        Action::NewWindow => open_new_window(s),
        Action::OpenTerminal { azterm } => open_terminal(s, azterm),
        Action::ClearHistory {
            recent,
            back_forward,
        } => {
            if recent {
                // The open place stays the one place visited.
                let here = s.place.clone();
                s.recent.clear();
                s.recent.push(here);
            }
            if back_forward {
                s.history.clear();
            }
            println!("AZDRIVE_DONE history {recent} {back_forward}");
            s.info(match (recent, back_forward) {
                (true, true) => "The places visited and the Back and Forward history are gone.",
                (true, false) => "The places visited are gone (the pins stay).",
                _ => "Back and Forward start from here.",
            });
        }
        Action::Shortcuts => {
            let was_open = azul_appkit::ui::settings_open(&s.kit);
            s.backstage = Some(0);
            azul_appkit::ui::open_settings(&s.kit, Some("Shortcuts"));
            crate::options_opened(s, was_open);
        }
        Action::TogglePin(place) => toggle_pin_of(info, app, s, place),
        Action::EasyAccessMenu => {
            let items = vec![
                able_item(app, s, "Pin to Quick access", Action::Pin),
                menu_item(
                    app,
                    "Add a folder as a drive...",
                    Action::AddLocalDrive,
                    false,
                ),
            ];
            open_menu_below(info, items);
        }
        Action::PropertiesMenu => {
            let items = vec![
                able_item(app, s, "Properties", Action::Properties),
                able_item(app, s, "Drive properties", Action::DriveProperties),
            ];
            open_menu_below(info, items);
        }
        Action::Print => print_selected(s),
        Action::HideSelected => hide_selected(info, app, s),
    }
}

// ==== File menu, Print, Hide ====

/// An app of the Azlin family next to AzDrive's own binary (`AzTerm`), if it is installed there.
pub(crate) fn sibling_app(name: &str) -> Option<PathBuf> {
    let me = std::env::current_exe().ok()?;
    let dir = me.parent()?;
    let file = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let path = dir.join(file);
    path.is_file().then_some(path)
}

/// File > Open new window: another AzDrive (a process of its own, as Explorer's "Open new
/// process") at the open place, with this run's switches - the place in `--open`.
fn open_new_window(s: &mut DriveState) {
    let Ok(me) = std::env::current_exe() else {
        s.error("AzDrive cannot find its own program to open another window.");
        return;
    };
    let path = crate::window_title(s)
        .trim_end_matches(" - AzDrive")
        .to_string();
    let args = crate::args::new_window_args(std::env::args().skip(1), &path);
    match std::process::Command::new(me).args(&args).spawn() {
        Ok(_) => {
            println!("AZDRIVE_NEW_WINDOW {path}");
            s.info(format!("Another window opens at \"{path}\"."));
        }
        Err(e) => s.error(format!("The new window could not be opened: {e}")),
    }
}

/// File > Open terminal here: the system's terminal (or AzTerm) in the open folder.
fn open_terminal(s: &mut DriveState, azterm: bool) {
    let Some(dir) = s
        .current_drive()
        .and_then(|i| s.local_dir(i, &s.prefix().to_string()))
    else {
        return;
    };
    let spawned = if azterm {
        match sibling_app("AzTerm") {
            Some(program) => std::process::Command::new(program)
                .current_dir(&dir)
                .spawn()
                .map(|_| ()),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "AzTerm is not installed next to AzDrive",
            )),
        }
    } else {
        system_terminal(&dir)
    };
    match spawned {
        Ok(()) => {
            println!("AZDRIVE_DONE terminal {}", dir.display());
            s.info(format!("A terminal opens in {}.", dir.display()));
        }
        Err(e) => s.error(format!("The terminal could not be opened: {e}")),
    }
}

/// The system's terminal in `dir`: Terminal on macOS, a new console on Windows, the desktop's
/// terminal emulator elsewhere (the Debian alternative first, then the common ones).
fn system_terminal(dir: &std::path::Path) -> std::io::Result<()> {
    use std::process::Command;
    if cfg!(target_os = "macos") {
        return Command::new("open")
            .arg("-a")
            .arg("Terminal")
            .arg(dir)
            .spawn()
            .map(|_| ());
    }
    if cfg!(windows) {
        return Command::new("cmd")
            .args(["/C", "start", "cmd"])
            .current_dir(dir)
            .spawn()
            .map(|_| ());
    }
    let mut last = std::io::Error::new(std::io::ErrorKind::NotFound, "no terminal emulator");
    for program in [
        "x-terminal-emulator",
        "gnome-terminal",
        "konsole",
        "xfce4-terminal",
        "xterm",
    ] {
        match Command::new(program).current_dir(dir).spawn() {
            Ok(_) => return Ok(()),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// Share > Print: the selected files to this computer's default printer (`lp`, the printing
/// system's own command on macOS and Linux; the file's app's print verb on Windows).
fn print_selected(s: &mut DriveState) {
    let Some(index) = s.current_drive() else {
        return;
    };
    let Some(root) = s.local_root(index) else {
        return;
    };
    let paths: Vec<PathBuf> = s
        .selected_entries()
        .iter()
        .filter(|e| !e.is_folder)
        .map(|e| crate::jobs::path_in(&root, &e.key))
        .collect();
    if paths.is_empty() {
        return;
    }
    let result = if cfg!(windows) {
        paths.iter().try_for_each(|path| {
            std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    "Start-Process",
                    "-Verb",
                    "Print",
                    "-FilePath",
                ])
                .arg(path)
                .spawn()
                .map(|_| ())
        })
    } else {
        std::process::Command::new("lp")
            .args(&paths)
            .spawn()
            .map(|_| ())
    };
    match result {
        Ok(()) => {
            println!("AZDRIVE_DONE printed {}", paths.len());
            s.success(format!(
                "Sent {} to the printer.",
                browse::counted(paths.len(), "file", "files")
            ));
        }
        Err(e) => s.error(format!("Nothing could be printed: {e}")),
    }
}

/// View > Hide selected items: an item whose name starts with a dot is hidden (the trash, the
/// dotfiles), so hiding renames the selected items with a leading dot - and when they all are
/// hidden already, without it. Each is a rename Ctrl+Z takes back.
fn hide_selected(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let entries: Vec<browse::Entry> = s.selected_entries().into_iter().cloned().collect();
    let unhide = entries.iter().all(browse::Entry::is_hidden);
    let Some(drive_id) = s.current_drive_id() else {
        return;
    };
    let Some(drive) = open_current(s) else {
        return;
    };
    let mut renamed = 0;
    for entry in entries {
        let name = if unhide {
            entry.name.trim_start_matches('.').to_string()
        } else if entry.is_hidden() {
            continue;
        } else {
            format!(".{}", entry.name)
        };
        if name.is_empty() || s.entries.iter().any(|e| e.name == name) {
            continue; // nothing left of the name, or the name is taken
        }
        let parent = fileops::parent_of(&entry.key);
        let to = if entry.is_folder {
            format!("{parent}{name}/")
        } else {
            format!("{parent}{name}")
        };
        spawn(
            info,
            app,
            s,
            Job::Rename {
                drive: drive.clone(),
                drive_id: drive_id.clone(),
                from: entry.key,
                to,
            },
        );
        renamed += 1;
    }
    if renamed > 0 && !unhide && !s.settings.show_hidden {
        s.info(format!(
            "Hid {} (View > Hidden items shows them).",
            browse::counted(renamed, "item", "items")
        ));
    }
}

/// Pins the folder `place` to Quick access, or unpins it (File > Frequent places' pins).
fn toggle_pin_of(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, place: Place) {
    let Place::Folder { drive, prefix } = place.clone() else {
        return;
    };
    if s.settings.is_pinned(&drive, &prefix) {
        s.settings
            .pinned
            .retain(|p| !(p.drive == drive && p.prefix == prefix));
    } else {
        let name = s.place_title(&place);
        s.settings.pinned.push(model::Pinned {
            drive,
            prefix,
            name,
        });
    }
    println!("AZDRIVE_DONE pinned {}", s.settings.pinned.len());
    save_settings(info, app, s);
}

// ==== Keyboard ====

/// Explorer's key from azul's key code.
fn key_of(code: VirtualKeyCode) -> Key {
    use VirtualKeyCode as V;
    let letter = |c: char| Key::Char(c);
    match code {
        V::Return | V::NumpadEnter => Key::Enter,
        V::Back => Key::Back,
        V::Delete => Key::Delete,
        V::Escape => Key::Escape,
        V::Tab => Key::Tab,
        V::Space => Key::Space,
        V::Up => Key::Up,
        V::Down => Key::Down,
        V::Left => Key::Left,
        V::Right => Key::Right,
        V::Home => Key::Home,
        V::End => Key::End,
        V::PageUp => Key::PageUp,
        V::PageDown => Key::PageDown,
        V::F2 => Key::F2,
        V::F3 => Key::F3,
        V::F5 => Key::F5,
        V::F10 => Key::F10,
        V::Apps => Key::Apps,
        V::A => letter('a'),
        V::B => letter('b'),
        V::C => letter('c'),
        V::D => letter('d'),
        V::E => letter('e'),
        V::F => letter('f'),
        V::G => letter('g'),
        V::H => letter('h'),
        V::I => letter('i'),
        V::J => letter('j'),
        V::K => letter('k'),
        V::L => letter('l'),
        V::M => letter('m'),
        V::N => letter('n'),
        V::O => letter('o'),
        V::P => letter('p'),
        V::Q => letter('q'),
        V::R => letter('r'),
        V::S => letter('s'),
        V::T => letter('t'),
        V::U => letter('u'),
        V::V => letter('v'),
        V::W => letter('w'),
        V::X => letter('x'),
        V::Y => letter('y'),
        V::Z => letter('z'),
        V::Key0 | V::Numpad0 => letter('0'),
        V::Key1 | V::Numpad1 => letter('1'),
        V::Key2 | V::Numpad2 => letter('2'),
        V::Key3 | V::Numpad3 => letter('3'),
        V::Key4 | V::Numpad4 => letter('4'),
        V::Key5 | V::Numpad5 => letter('5'),
        V::Key6 | V::Numpad6 => letter('6'),
        V::Key7 | V::Numpad7 => letter('7'),
        V::Key8 | V::Numpad8 => letter('8'),
        V::Key9 | V::Numpad9 => letter('9'),
        _ => Key::Other,
    }
}

/// Whether the keyboard focus is in a text field (the path, the search,
/// a rename, a form): its keys are its own.
fn in_text_field(info: &CallbackInfo) -> bool {
    let Some(mut node) = info.get_focused_node().into_option() else {
        return false;
    };
    for _ in 0..6 {
        let classes = info.get_node_classes(node);
        if classes
            .as_slice()
            .iter()
            .any(|c| c.as_str().contains("text-input"))
        {
            return true;
        }
        match info.get_parent(node).into_option() {
            Some(parent) => node = parent,
            None => return false,
        }
    }
    false
}

/// Explorer's keyboard, at window level.
pub(crate) extern "C" fn on_key_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(code) = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option()
    else {
        return Update::DoNothing;
    };
    let m = info.get_key_modifiers();
    let mods = Mods {
        shift: m.shift,
        ctrl: m.primary_down(),
        alt: m.alt,
    };
    // azul-appkit's keys first: Mod+, opens the Options, F1 at the keyboard shortcuts, Escape
    // closes them.
    let kit = data.downcast_ref::<DriveState>().map(|s| s.kit.clone());
    if let Some(kit) = kit {
        let was_open = azul_appkit::ui::settings_open(&kit);
        if let Some(update) = azul_appkit::ui::handle_key(&kit, &mut info) {
            if let Some(mut s) = data.downcast_mut::<DriveState>() {
                let open = azul_appkit::ui::settings_open(&kit);
                s.backstage = open.then_some(0);
                if open {
                    crate::options_opened(&mut s, was_open);
                }
            }
            return update;
        }
    }
    if in_text_field(&info) {
        return Update::DoNothing;
    }
    // The source list's own keys while the keyboard is in it: the arrows walk its rows, Enter
    // opens one (the rest - F5, Backspace, Ctrl+C ... - stays the window's).
    if let Some(update) = crate::ui_sidebar::on_sidebar_key(&mut data, &mut info, key_of(code), mods)
    {
        return update;
    }
    let Some(command) = keys::command_for(key_of(code), mods) else {
        return Update::DoNothing;
    };
    if grid_handles(command) && in_icon_grid(&info) {
        return Update::DoNothing;
    }
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    if s.popup.is_some() || s.backstage_shown().is_some() {
        if command == Command::Escape {
            close_popup(&mut info, &app, s);
            s.backstage = None;
            return Update::RefreshDom;
        }
        return Update::DoNothing;
    }
    if s.renaming.is_some() {
        return Update::DoNothing;
    }
    info.prevent_default();
    run_command(&mut info, &app, s, command);
    Update::RefreshDom
}

/// The window was resized: the grid's rows for the arrow keys; a new width rebuilds the
/// window (the address bar folds the crumbs that no longer fit into its « menu, the
/// folder view lays its lines out for the new width, the icon grid draws exactly its new
/// viewport).
pub(crate) extern "C" fn on_resized(mut data: RefAny, info: CallbackInfo) -> Update {
    let width = info.get_current_window_state().size.dimensions.width;
    let Some(mut s) = data.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    let changed = (s.window_width - width).abs() >= 1.0;
    s.window_width = width;
    if changed || crate::ui_view::uses_icon_grid(&s) {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// A transfer that runs this long shows Explorer's progress dialog (once).
pub(crate) const PROGRESS_DIALOG_AFTER_MS: u64 = 2_000;

/// A progress message arrived: a transfer that has run [`PROGRESS_DIALOG_AFTER_MS`] shows the
/// progress dialog (azul's ProgressDialog over the queue), unless another dialog is open or it
/// was shown for this transfer already. It closes by itself when the queue is done.
pub(crate) fn show_progress_when_long(s: &mut DriveState) {
    if s.popup.is_some() {
        return;
    }
    if let Some(id) = s.queue.wants_progress_dialog(now_ms(), PROGRESS_DIALOG_AFTER_MS) {
        s.queue.mark_dialog_shown(id);
        s.popups_opened += 1;
        s.popup = Some(Popup::Transfers { auto: true });
    }
}

/// Milliseconds since 1970 (type-ahead's clock, the transfers' start times).
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Seconds since 1970 (azul-storage's clock, DEDUP_OFFICE D23).
pub(crate) fn now_secs() -> u64 {
    azul_storage::time::now_unix()
}

/// Runs a keyboard command.
pub(crate) fn run_command(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    command: Command,
) {
    match command {
        Command::Open => run_action(info, app, s, Action::Open),
        Command::Properties => show_properties(info, app, s),
        Command::Up => {
            if let Some(place) = place_up(&s.place) {
                go(info, app, s, place, true);
            }
        }
        Command::Back => {
            let current = s.place.clone();
            if let Some(place) = s.history.back(current) {
                go(info, app, s, place, false);
            }
        }
        Command::Forward => {
            let current = s.place.clone();
            if let Some(place) = s.history.forward(current) {
                go(info, app, s, place, false);
            }
        }
        Command::Refresh => refresh(info, app, s),
        Command::Search => focus_search(info),
        Command::Escape => {
            s.type_ahead.clear();
            if s.editing_path {
                s.editing_path = false;
            } else if !s.search.is_empty() {
                s.search.clear();
            } else {
                s.selection.clear();
                s.print_selection();
            }
        }
        Command::Rename => run_action(info, app, s, Action::Rename),
        Command::Delete => run_action(info, app, s, Action::Delete),
        Command::DeletePermanently => run_action(info, app, s, Action::DeletePermanently),
        Command::Copy => run_action(info, app, s, Action::Copy),
        Command::Cut => run_action(info, app, s, Action::Cut),
        Command::Paste => run_action(info, app, s, Action::Paste),
        Command::SelectAll => run_action(info, app, s, Action::SelectAll),
        Command::Undo => run_action(info, app, s, Action::Undo),
        Command::NewFolder => run_action(info, app, s, Action::NewFolder),
        Command::ContextMenu => {
            let menu = context_menu(app, s);
            info.open_menu(menu);
        }
        Command::Move { step, extend, keep } => move_focus(info, app, s, step, extend, keep),
        Command::ToggleFocused => {
            s.selection.toggle_focused();
            s.print_selection();
        }
        Command::TypeAhead(c) => type_ahead(info, app, s, c),
        Command::Layout(n) => {
            if let Some(layout) = ViewLayout::ALL.get(usize::from(n).saturating_sub(1)) {
                set_layout(info, app, s, *layout);
            }
        }
        Command::PreviewPane => run_action(info, app, s, Action::Toggle(Toggle::PreviewPane)),
        Command::DetailsPane => run_action(info, app, s, Action::Toggle(Toggle::DetailsPane)),
    }
}

/// Whether the keyboard focus is on the icon layouts' grid (azul's IconGrid), which moves,
/// selects, opens and types ahead by itself - and reports it as its events.
fn in_icon_grid(info: &CallbackInfo) -> bool {
    let Some(mut node) = info.get_focused_node().into_option() else {
        return false;
    };
    for _ in 0..4 {
        let classes = info.get_node_classes(node);
        if classes
            .as_slice()
            .iter()
            .any(|c| c.as_str() == "__azul-native-icon-grid")
        {
            return true;
        }
        match info.get_parent(node).into_option() {
            Some(parent) => node = parent,
            None => return false,
        }
    }
    false
}

/// The keys the focused icon grid handles itself (the window's keyboard leaves them).
fn grid_handles(command: Command) -> bool {
    matches!(
        command,
        Command::Move { .. }
            | Command::ToggleFocused
            | Command::TypeAhead(_)
            | Command::Open
            | Command::SelectAll
            | Command::ContextMenu
    )
}

/// Ctrl+F: the address bar's search box gets the focus.
fn focus_search(info: &mut CallbackInfo) {
    let hit = info.get_hit_node();
    let host = info.get_node_id_by_id_attribute(hit.dom, AzString::from("shell-address-bar"));
    if host.into_raw() == 0 {
        return;
    }
    let start = DomNodeId {
        dom: hit.dom,
        node: host,
    };
    if let Some(search) = find_class(info, start, "__azul-native-address-bar-search", 16) {
        if let Some(input) = info.get_first_child(search).into_option() {
            info.set_focus(FocusTarget::Id(input));
        }
    }
}

/// The first node under `node` (itself included) carrying `class`.
pub(crate) fn find_class(
    info: &CallbackInfo,
    node: DomNodeId,
    class: &str,
    depth: usize,
) -> Option<DomNodeId> {
    let classes = info.get_node_classes(node);
    if classes.as_slice().iter().any(|c| c.as_str() == class) {
        return Some(node);
    }
    if depth == 0 {
        return None;
    }
    let mut child = info.get_first_child(node).into_option();
    while let Some(c) = child {
        if let Some(found) = find_class(info, c, class, depth - 1) {
            return Some(found);
        }
        child = info.get_next_sibling(c).into_option();
    }
    None
}

/// An arrow / Home / End / Page key: the focus (and the selection) moves.
fn move_focus(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    step: Step,
    extend: bool,
    keep: bool,
) {
    let columns = s.grid_columns();
    match s.place {
        Place::ThisPc => {
            if s.slots.is_empty() {
                return;
            }
            let last = s.slots.len() as isize - 1;
            let at = s.selected_drive.map_or(-1, |i| i as isize);
            let to = (at + step.delta(columns, 10).signum()).clamp(0, last);
            s.selected_drive = Some(to as usize);
        }
        Place::QuickAccess => {
            if s.settings.pinned.is_empty() {
                return;
            }
            let last = s.settings.pinned.len() as isize - 1;
            let at = s.selected_pin.map_or(-1, |i| i as isize);
            let to = (at + step.delta(columns, 10).signum()).clamp(0, last);
            s.selected_pin = Some(to as usize);
        }
        Place::Folder { .. } => {
            // The order the view shows (grouped: group after group).
            let keys: Vec<String> = ui_view::shown_order(s)
                .into_iter()
                .map(|e| e.key.clone())
                .collect();
            let order: Vec<&str> = keys.iter().map(String::as_str).collect();
            s.selection
                .step(&order, step.delta(columns, 10), extend, keep);
            s.print_selection();
            request_preview(info, app, s);
            reveal_focus(info, s);
        }
    }
}

/// The virtual view scrolls the item the keyboard is on into view.
fn reveal_focus(info: &mut CallbackInfo, s: &mut DriveState) {
    if let Some(key) = s.selection.focus().map(str::to_string) {
        ui_view::reveal_item(info, s, &key);
    }
}

/// A letter typed: the next name starting with what was typed.
fn type_ahead(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, c: char) {
    let query = s.type_ahead.push(c, now_ms());
    match s.place {
        Place::ThisPc => {
            let names: Vec<&str> = s.slots.iter().map(|slot| slot.entry.name.as_str()).collect();
            if let Some(i) = model::type_ahead_match(&names, &query, s.selected_drive) {
                s.selected_drive = Some(i);
            }
        }
        Place::QuickAccess => {
            let names: Vec<&str> = s.settings.pinned.iter().map(|p| p.name.as_str()).collect();
            if let Some(i) = model::type_ahead_match(&names, &query, s.selected_pin) {
                s.selected_pin = Some(i);
            }
        }
        Place::Folder { .. } => {
            let show_extensions = s.settings.show_extensions;
            let entries = s.visible_entries();
            let names: Vec<String> = entries
                .iter()
                .map(|e| e.display_name(show_extensions))
                .collect();
            let keys: Vec<String> = entries.iter().map(|e| e.key.clone()).collect();
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            let current = s
                .selection
                .focus()
                .and_then(|f| keys.iter().position(|k| k == f));
            if let Some(i) = model::type_ahead_match(&refs, &query, current) {
                let key = keys[i].clone();
                s.selection.click(&key);
                s.print_selection();
                request_preview(info, app, s);
                reveal_focus(info, s);
            }
        }
    }
}

// ==== Selection and opening ====

/// A click on an item: alone, Ctrl+click toggles, Shift+click ranges.
pub(crate) fn click_item(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    key: &str,
    toggle: bool,
    range: bool,
) {
    if s.renaming.as_ref().is_some_and(|r| r.key != key) {
        commit_rename(info, app, s);
    }
    let keys = s.visible_keys();
    let order: Vec<&str> = keys.iter().map(String::as_str).collect();
    match (toggle, range) {
        (true, true) => s.selection.add_range(key, &order),
        (false, true) => s.selection.extend(key, &order),
        (true, false) => s.selection.toggle(key),
        (false, false) => s.selection.click(key),
    }
    s.type_ahead.clear();
    s.print_selection();
    request_preview(info, app, s);
}

/// Select all.
fn select_all(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let keys = s.visible_keys();
    let order: Vec<&str> = keys.iter().map(String::as_str).collect();
    s.selection.select_all(&order);
    s.print_selection();
    request_preview(info, app, s);
}

/// Enter / double-click / Open: a folder opens, a file goes to the OS's app
/// (a cloud file is fetched first), a drive or pin opens.
pub(crate) fn open_selected(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    match &s.place {
        Place::ThisPc => {
            if let Some(index) = s.selected_drive {
                let id = s.slots[index].entry.id.clone();
                go(info, app, s, Place::folder(&id, ""), true);
            }
        }
        Place::QuickAccess => {
            if let Some(pin) = s.selected_pin.and_then(|i| s.settings.pinned.get(i)).cloned() {
                go(info, app, s, Place::folder(&pin.drive, &pin.prefix), true);
            }
        }
        Place::Folder { .. } => {
            let focus = s.selection.focus().map(str::to_string);
            let key = match (s.selection.single(), focus) {
                (Some(key), _) => Some(key.to_string()),
                (None, Some(focus)) if s.selection.contains(&focus) => Some(focus),
                _ => s.selection.keys().first().cloned(),
            };
            if let Some(key) = key {
                activate(info, app, s, &key);
            }
        }
    }
}

/// Opens one item: a folder in the window, a file with the OS's app.
pub(crate) fn activate(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, key: &str) {
    let Some(entry) = s.entry(key).cloned() else {
        return;
    };
    let Some(drive_id) = s.current_drive_id() else {
        return;
    };
    if entry.is_folder {
        go(info, app, s, Place::folder(&drive_id, &entry.key), true);
        return;
    }
    let Some(drive) = open_current(s) else {
        return;
    };
    let folder = s.open_dir.join(&drive_id);
    s.info(format!("Opening \"{}\"...", entry.name));
    spawn(
        info,
        app,
        s,
        Job::Open {
            drive,
            key: entry.key,
            size: entry.size,
            folder,
        },
    );
}

// ==== Clipboard ====

/// Ctrl+C / Ctrl+X: the selected items wait for a paste.
fn copy_selected(s: &mut DriveState, cut: bool) {
    let Some(drive) = s.current_drive_id() else {
        return;
    };
    let items = s.selected_items();
    if items.is_empty() {
        return;
    }
    println!(
        "AZDRIVE_CLIPBOARD {} {}",
        if cut { "cut" } else { "copy" },
        items.len()
    );
    s.info(format!(
        "{} {} item(s): Ctrl+V pastes them into the open folder.",
        if cut { "Cut" } else { "Copied" },
        items.len()
    ));
    s.clipboard = Some(ClipboardItems { drive, items, cut });
}

/// Ctrl+V: the clipboard's items into the open folder (a cut moves them).
fn paste(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(clip) = s.clipboard.clone() else {
        return;
    };
    let Some(target_id) = s.current_drive_id() else {
        return;
    };
    let prefix = s.prefix().to_string();
    let kind = if clip.cut {
        TransferKind::Move
    } else {
        TransferKind::Copy
    };
    enqueue_transfer(info, app, s, kind, &clip.drive, clip.items, &target_id, &prefix, None);
    if clip.cut {
        // Explorer: what was cut is pasted once.
        s.clipboard = None;
    }
}

/// Where an item is, as text: the file's path on this computer, or the
/// `s3://bucket/key` of a cloud object.
pub(crate) fn item_location(s: &DriveState, drive_id: &str, item_key: &str) -> String {
    let Some(index) = s.slot_index(drive_id) else {
        return item_key.to_string();
    };
    match &s.slots[index].entry.location {
        DriveLocation::Local { root } => {
            let mut path = PathBuf::from(root);
            for segment in item_key.split('/').filter(|p| !p.is_empty()) {
                path.push(segment);
            }
            path.display().to_string()
        }
        DriveLocation::S3 { bucket, .. } => format!("s3://{bucket}/{item_key}"),
    }
}

/// Copy path: the selected items' locations on the system clipboard.
fn copy_path(info: &mut CallbackInfo, s: &mut DriveState) {
    let Some(drive_id) = s.current_drive_id() else {
        return;
    };
    let text = s
        .selected_entries()
        .iter()
        .map(|e| item_location(s, &drive_id, &e.key))
        .collect::<Vec<_>>()
        .join("\n");
    let lines = text.lines().count();
    info.set_clipboard_content(ClipboardContent {
        plain_text: AzString::from(text),
        styled_runs: StyledTextRunVec::create(),
        html: OptionString::None,
    });
    println!("AZDRIVE_DONE copied-path {lines}");
    s.info(format!("Copied the path of {lines} item(s)."));
}

// ==== The transfer queue ====

/// "Copying 3 items to docs".
fn transfer_label(kind: TransferKind, items: &[SourceItem], target: &str) -> String {
    let what = match items {
        [one] => format!("\"{}\"", key::last_segment(&one.key)),
        many => format!("{} items", many.len()),
    };
    format!("{} {what} to {target}", kind.verb())
}

/// Queues copying (or moving) `items` of drive `source_id` into
/// `target_prefix` of drive `target_id`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn enqueue_transfer(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    kind: TransferKind,
    source_id: &str,
    items: Vec<SourceItem>,
    target_id: &str,
    target_prefix: &str,
    auto: Option<ConflictChoice>,
) {
    let Some(source) = open_drive(s, source_id) else {
        s.error("The items' drive is gone.");
        return;
    };
    let Some(target) = open_drive(s, target_id) else {
        return;
    };
    let target_name = s.place_title(&Place::folder(target_id, target_prefix));
    enqueue_with(
        info,
        app,
        s,
        kind,
        (source_id.to_string(), source),
        items,
        (target_id.to_string(), target),
        target_prefix,
        &target_name,
        auto,
    );
}

/// Queues a transfer between two opened drives (an OS folder for uploads
/// and downloads).
#[allow(clippy::too_many_arguments)]
fn enqueue_with(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    kind: TransferKind,
    source: (String, Arc<dyn Drive>),
    items: Vec<SourceItem>,
    target: (String, Arc<dyn Drive>),
    target_prefix: &str,
    target_name: &str,
    auto: Option<ConflictChoice>,
) {
    if items.is_empty() {
        return;
    }
    let label = transfer_label(kind, &items, target_name);
    let id = s.queue.push(label.clone());
    let same_drive = source.0 == target.0;
    s.transfers.insert(
        id,
        TransferJob {
            kind,
            source_id: source.0,
            target_id: target.0,
            source: source.1,
            target: target.1,
            items,
            target_prefix: target_prefix.to_string(),
            same_drive,
            plan: None,
            cancel: Arc::new(AtomicBool::new(false)),
            auto,
        },
    );
    s.info(format!("{label}..."));
    pump_queue(info, app, s);
}

/// Starts the next transfer when none runs: its plan first.
pub(crate) fn pump_queue(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(id) = s.queue.next_to_start() else {
        return;
    };
    let Some(job) = s.transfers.get(&id) else {
        s.queue.finish(id, Some(String::from("the transfer was lost")));
        return;
    };
    let plan_job = Job::Plan {
        id,
        source: job.source.clone(),
        items: job.items.clone(),
        target: job.target.clone(),
        target_prefix: job.target_prefix.clone(),
        same_drive: job.same_drive,
        kind: job.kind,
    };
    s.queue.start(id, now_ms());
    spawn(info, app, s, plan_job);
}

/// Whether transfer `id` was cancelled.
fn is_cancelled(s: &DriveState, id: u64) -> bool {
    s.queue
        .jobs()
        .iter()
        .any(|j| j.id == id && j.state == fileops::JobState::Cancelled)
}

/// A transfer's plan is back: ask about the conflicts, or run it.
pub(crate) fn transfer_planned(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    id: u64,
    result: Result<Plan, DriveError>,
) {
    if is_cancelled(s, id) {
        s.transfers.remove(&id);
        pump_queue(info, app, s);
        return;
    }
    let mut plan = match result {
        Ok(plan) => plan,
        Err(e) => {
            s.transfers.remove(&id);
            s.queue.finish(id, Some(e.to_string()));
            println!("AZDRIVE_TRANSFER {id} failed 0");
            s.error(format!("The transfer cannot start: {e}"));
            pump_queue(info, app, s);
            return;
        }
    };
    let Some(job) = s.transfers.get_mut(&id) else {
        return;
    };
    if let Some(choice) = job.auto {
        for i in plan.conflicts() {
            plan.choose(i, choice);
        }
    }
    println!(
        "AZDRIVE_TRANSFER {id} planned {}",
        plan.files.len() + plan.moves.len()
    );
    let empty = plan.is_empty();
    let conflicts = plan.unresolved_count();
    job.plan = Some(plan);
    if empty {
        s.transfers.remove(&id);
        s.queue.finish(id, None);
        println!("AZDRIVE_TRANSFER {id} done 0");
        s.info("Nothing to do: the items are where they would go.");
        pump_queue(info, app, s);
    } else if conflicts > 0 {
        println!("AZDRIVE_TRANSFER {id} conflict {conflicts}");
        s.popups_opened += 1;
        s.popup = Some(Popup::Conflict {
            id,
            apply_all: false,
        });
    } else {
        run_planned(info, app, s, id);
    }
}

/// Runs transfer `id`'s (decided) plan on a thread.
fn run_planned(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, id: u64) {
    let Some(job) = s.transfers.get(&id) else {
        return;
    };
    let Some(plan) = job.plan.clone() else {
        return;
    };
    let run = Job::Run {
        id,
        plan,
        source: job.source.clone(),
        target: job.target.clone(),
        kind: job.kind,
        cancel: job.cancel.clone(),
    };
    spawn(info, app, s, run);
}

/// The conflict dialog's answer for the next conflict (or all of them).
pub(crate) fn resolve_conflict(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    choice: ConflictChoice,
) {
    let Some(Popup::Conflict { id, apply_all }) = s.popup.take() else {
        return;
    };
    let mut done = true;
    if let Some(plan) = s.transfers.get_mut(&id).and_then(|job| job.plan.as_mut()) {
        if apply_all {
            while let Some(i) = plan.unresolved() {
                plan.choose(i, choice);
            }
        } else if let Some(i) = plan.unresolved() {
            plan.choose(i, choice);
        }
        done = plan.unresolved().is_none();
    }
    if done {
        run_planned(info, app, s, id);
    } else {
        s.popup = Some(Popup::Conflict { id, apply_all });
    }
}

/// Cancel: a waiting transfer never starts, a running one stops before its
/// next file, one waiting for a conflict answer ends now.
pub(crate) fn cancel_transfer(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    id: u64,
) {
    if let Some(job) = s.transfers.get(&id) {
        job.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    let asking = matches!(s.popup, Some(Popup::Conflict { id: asked, .. }) if asked == id);
    let waiting = s
        .queue
        .jobs()
        .iter()
        .any(|j| j.id == id && j.state == fileops::JobState::Waiting);
    s.queue.cancel(id);
    if asking || waiting {
        if asking {
            s.popup = None;
        }
        s.transfers.remove(&id);
        println!("AZDRIVE_TRANSFER {id} cancelled 0");
        s.info("The transfer was cancelled.");
        pump_queue(info, app, s);
    }
}

/// A transfer ended: say how, refresh what changed, select what arrived,
/// remember a move for Ctrl+Z, start the next one.
pub(crate) fn transfer_ran(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    id: u64,
    report: TransferReport,
) {
    let job = s.transfers.remove(&id);
    let failed = report.failed.first().map(|(what, why)| {
        format!(
            "{} item(s) failed; \"{}\": {why}",
            report.failed.len(),
            key::last_segment(what)
        )
    });
    if !report.cancelled {
        s.queue.finish(id, failed.clone());
    }
    let state = if report.cancelled {
        "cancelled"
    } else if failed.is_some() {
        "failed"
    } else {
        "done"
    };
    println!("AZDRIVE_TRANSFER {id} {state} {}", report.done);
    match (&failed, report.cancelled) {
        (_, true) => s.info(format!("Cancelled after {} item(s).", report.done)),
        (Some(text), _) => s.error(text.clone()),
        (None, _) if report.skipped > 0 => s.success(format!(
            "Done: {} item(s), {} skipped.",
            report.done, report.skipped
        )),
        (None, _) => s.success(format!("Done: {} item(s).", report.done)),
    }
    if let Some(job) = job {
        crate::changed(info, app, s, &job.target_id, &job.target_prefix);
        if job.kind.removes_source() {
            let mut parents: Vec<String> = job
                .items
                .iter()
                .map(|item| fileops::parent_of(&item.key))
                .collect();
            parents.dedup();
            for parent in parents {
                crate::changed(info, app, s, &job.source_id, &parent);
            }
        }
        if let Some(plan) = &job.plan {
            if crate::showing(s, &job.target_id, &job.target_prefix) {
                s.selection.set(plan.tops.clone());
            }
            if job.kind == TransferKind::Move
                && job.same_drive
                && !plan.moves.is_empty()
                && plan.files.is_empty()
            {
                s.undo.push(UndoOp::Move {
                    drive: job.source_id.clone(),
                    pairs: plan
                        .moves
                        .iter()
                        .map(|(from, to)| (to.clone(), from.clone()))
                        .collect(),
                });
            }
        }
        if job.kind == TransferKind::Download && !report.cancelled && failed.is_none() {
            let folder = s.downloads.display().to_string();
            s.success(format!("Downloaded {} item(s) to {folder}.", report.done));
        }
    }
    if s.queue.is_idle() && s.queue.failed().is_empty() {
        s.queue.clear_finished();
    }
    pump_queue(info, app, s);
    // The progress dialog that opened by itself closes by itself once nothing runs or waits.
    if matches!(s.popup, Some(Popup::Transfers { auto: true })) && s.queue.is_idle() {
        s.popup = None;
    }
}

/// Move to / Copy to: Quick access's pins, the drives, "Choose location".
fn destination_menu(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    kind: TransferKind,
) {
    let items = destination_items(app, s, kind);
    open_menu_below(info, items);
}

/// The folders Move to / Copy to offer first: the ones visited last (Explorer's list), then the
/// pinned ones not among them - never the open folder (an item moved to where it is stays).
pub(crate) fn destination_places(s: &DriveState) -> Vec<(Place, String)> {
    /// Folders the list offers before the drives.
    const MAX: usize = 10;
    let mut places: Vec<(Place, String)> = Vec::new();
    let recent = s.recent.iter().map(|p| (p.clone(), s.place_title(p)));
    let pinned = s
        .settings
        .pinned
        .iter()
        .map(|pin| (Place::folder(&pin.drive, &pin.prefix), pin.name.clone()));
    for (place, name) in recent.chain(pinned) {
        if !matches!(place, Place::Folder { .. })
            || place == s.place
            || places.iter().any(|(p, _)| *p == place)
        {
            continue;
        }
        places.push((place, name));
        if places.len() == MAX {
            break;
        }
    }
    places
}

/// The entries of Move to / Copy to: the recent and pinned folders, the drives, and "Choose
/// location..." (the system's folder dialog).
fn destination_items(app: &RefAny, s: &DriveState, kind: TransferKind) -> Vec<MenuItem> {
    let wrap = |place: Place| match kind {
        TransferKind::Move => Action::MoveTo(place),
        _ => Action::CopyTo(place),
    };
    let mut items: Vec<MenuItem> = destination_places(s)
        .into_iter()
        .map(|(place, name)| menu_item(app, &name, wrap(place), false))
        .collect();
    if !items.is_empty() {
        items.push(MenuItem::Separator);
    }
    for slot in &s.slots {
        items.push(menu_item(
            app,
            &slot.entry.name,
            wrap(Place::folder(&slot.entry.id, "")),
            false,
        ));
    }
    items.push(MenuItem::Separator);
    items.push(menu_item(
        app,
        "Choose location...",
        Action::ChooseLocation(kind),
        false,
    ));
    items
}

/// The selected items into the folder `place`.
pub(crate) fn transfer_selection_to(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    kind: TransferKind,
    place: Place,
) {
    let Place::Folder { drive, prefix } = place else {
        s.warn("Choose a folder of a drive.");
        return;
    };
    let Some(source_id) = s.current_drive_id() else {
        return;
    };
    let items = s.selected_items();
    enqueue_transfer(info, app, s, kind, &source_id, items, &drive, &prefix, None);
}

/// The items dragged in the window, dropped on the folder `target_prefix`
/// of the open drive (Ctrl copies, else a move).
pub(crate) fn drop_on_folder(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    target_prefix: &str,
    copy: bool,
) {
    let Some(target_id) = s.current_drive_id() else {
        return;
    };
    drop_on_place(info, app, s, Place::folder(&target_id, target_prefix), copy);
}

/// The items dragged in the window, dropped on the folder `place` (any
/// drive; a folder of the navigation pane): within one drive a move (Ctrl
/// copies), across drives a copy.
pub(crate) fn drop_on_place(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    place: Place,
    copy: bool,
) {
    let Some((source_id, items)) = s.dragging.take() else {
        return;
    };
    let Place::Folder {
        drive: target_id,
        prefix: target_prefix,
    } = place
    else {
        s.warn("Drop the items on a folder of a drive.");
        return;
    };
    // A folder never lands on itself, and an item dropped into the folder it is in stays
    // there (Explorer does nothing: a drag let go over its own folder's view).
    let items: Vec<SourceItem> = items
        .into_iter()
        .filter(|item| {
            !(source_id == target_id
                && (item.key == target_prefix || key::parent_prefix(&item.key) == target_prefix))
        })
        .collect();
    if items.is_empty() {
        return;
    }
    let kind = if copy || source_id != target_id {
        TransferKind::Copy
    } else {
        TransferKind::Move
    };
    enqueue_transfer(info, app, s, kind, &source_id, items, &target_id, &target_prefix, None);
}

// ==== Upload and download ====

/// Uploads files and folders of this computer into the open folder: one
/// transfer per folder they sit in (a `LocalDrive` on it is the source).
pub(crate) fn upload_paths(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    paths: Vec<PathBuf>,
) {
    let Some(target_id) = s.current_drive_id() else {
        s.warn("Open a folder of a drive to upload into.");
        return;
    };
    let Some(target) = open_current(s) else {
        return;
    };
    let target_prefix = s.prefix().to_string();
    let target_name = s.place_name();
    let mut groups: Vec<(PathBuf, Vec<SourceItem>)> = Vec::new();
    for path in paths {
        let Some(parent) = path.parent().map(PathBuf::from) else {
            continue;
        };
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        if let Err(why) = fileops::check_name(&name) {
            s.error(format!("\"{name}\" cannot be uploaded: {why}"));
            continue;
        }
        let is_folder = path.is_dir();
        let item = SourceItem {
            key: if is_folder { format!("{name}/") } else { name },
            is_folder,
            size: std::fs::metadata(&path)
                .ok()
                .filter(|m| m.is_file())
                .map(|m| m.len()),
        };
        match groups.iter_mut().find(|(p, _)| *p == parent) {
            Some((_, items)) => items.push(item),
            None => groups.push((parent, vec![item])),
        }
    }
    for (parent, items) in groups {
        // An OS folder, not the data tree: no `.azlin/` bookkeeping there.
        let source: Arc<dyn Drive> = Arc::new(LocalDrive::without_manifest(parent.clone()));
        enqueue_with(
            info,
            app,
            s,
            TransferKind::Upload,
            (format!("os:{}", parent.display()), source),
            items,
            (target_id.clone(), target.clone()),
            &target_prefix,
            &target_name,
            None,
        );
    }
}

/// The OS's file dialog picked files to upload.
extern "C" fn on_upload_picked(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenMultiResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let paths: Vec<PathBuf> = picked
        .paths
        .as_slice()
        .iter()
        .map(|p| PathBuf::from(p.inner.as_str()))
        .collect();
    if paths.is_empty() {
        return Update::DoNothing; // cancelled
    }
    with_state(&mut data, &mut info, |info, app, s| {
        upload_paths(info, app, s, paths)
    })
}

/// Files dropped from the OS onto the window: uploaded into the open folder.
pub(crate) extern "C" fn on_dropped_file(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let paths: Vec<PathBuf> = info
        .get_dropped_files()
        .as_slice()
        .iter()
        .map(|p| PathBuf::from(p.as_str()))
        .collect();
    if paths.is_empty() {
        return Update::DoNothing;
    }
    with_state(&mut data, &mut info, |info, app, s| {
        upload_paths(info, app, s, paths)
    })
}

/// Download: the selected items into the Downloads folder (a taken name
/// keeps both, as a browser does).
fn download_selected(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(source_id) = s.current_drive_id() else {
        return;
    };
    let Some(source) = open_current(s) else {
        return;
    };
    let items = s.selected_items();
    let folder = s.downloads.clone();
    let target: Arc<dyn Drive> = Arc::new(LocalDrive::without_manifest(folder.clone()));
    enqueue_with(
        info,
        app,
        s,
        TransferKind::Download,
        (source_id, source),
        items,
        (String::from("os:downloads"), target),
        "",
        &folder.display().to_string(),
        Some(ConflictChoice::KeepBoth),
    );
}

// ==== Delete, rename, new, undo ====

/// Delete: into the trash folder on a local drive (Ctrl+Z brings it back);
/// for good on a cloud drive, in the trash itself and with Shift+Delete -
/// after a question unless the user turned it off (a cloud drive always asks).
fn delete_selected(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, permanent: bool) {
    let Some(drive_id) = s.current_drive_id() else {
        return;
    };
    let items = s.selected_items();
    if items.is_empty() {
        return;
    }
    let in_trash = items.iter().all(|i| fileops::is_in_trash(&i.key));
    let local = s.is_local_drive(&drive_id);
    if local && !permanent && !in_trash {
        s.trash_serial += 1;
        let stamp = fileops::trash_stamp(now_secs(), s.trash_serial);
        let Some(drive) = open_current(s) else {
            return;
        };
        s.info(format!(
            "Moving {} item(s) to the trash folder...",
            items.len()
        ));
        spawn(
            info,
            app,
            s,
            Job::Delete {
                drive,
                drive_id,
                items,
                stamp: Some(stamp),
            },
        );
        return;
    }
    if !local || s.settings.confirm_delete {
        s.popups_opened += 1;
        s.popup = Some(Popup::ConfirmDelete { drive_id, items });
        return;
    }
    delete_for_good(info, app, s, drive_id, items);
}

/// The delete dialog said yes.
pub(crate) fn confirm_delete(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(Popup::ConfirmDelete { drive_id, items }) = s.popup.take() else {
        return;
    };
    delete_for_good(info, app, s, drive_id, items);
}

fn delete_for_good(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: String,
    items: Vec<SourceItem>,
) {
    let Some(drive) = open_drive(s, &drive_id) else {
        return;
    };
    s.info(format!("Deleting {} item(s)...", items.len()));
    spawn(
        info,
        app,
        s,
        Job::Delete {
            drive,
            drive_id,
            items,
            stamp: None,
        },
    );
}

/// F2: the selected item's name becomes a text field (the extension stays
/// out of it while extensions are hidden).
fn start_rename(s: &mut DriveState) {
    let Some(entry) = s.single_selected().cloned() else {
        return;
    };
    println!("AZDRIVE_RENAMING {}", entry.key);
    s.renaming = Some(Renaming {
        text: entry.display_name(s.settings.show_extensions),
        is_folder: entry.is_folder,
        key: entry.key,
    });
}

/// Enter (or a click elsewhere) in the rename field: the new name.
pub(crate) fn commit_rename(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(renaming) = s.renaming.take() else {
        return;
    };
    let Some(entry) = s.entry(&renaming.key).cloned() else {
        return;
    };
    let mut name = renaming.text.trim().to_string();
    if !s.settings.show_extensions && !entry.is_folder {
        if let Some(ext) = browse::extension_of(&entry.name) {
            name = format!("{name}.{ext}");
        }
    }
    if name == entry.name {
        return;
    }
    if let Err(why) = fileops::check_name(&name) {
        s.error(why);
        s.renaming = Some(renaming);
        return;
    }
    if s
        .entries
        .iter()
        .any(|e| e.key != entry.key && e.name.eq_ignore_ascii_case(&name))
    {
        s.error(format!(
            "There is already an item named \"{name}\" in this folder."
        ));
        s.renaming = Some(renaming);
        return;
    }
    let parent = fileops::parent_of(&entry.key);
    let to = if entry.is_folder {
        format!("{parent}{name}/")
    } else {
        format!("{parent}{name}")
    };
    let Some(drive_id) = s.current_drive_id() else {
        return;
    };
    let Some(drive) = open_current(s) else {
        return;
    };
    spawn(
        info,
        app,
        s,
        Job::Rename {
            drive,
            drive_id,
            from: entry.key,
            to,
        },
    );
}

/// New folder / New item: the first free name, then rename it in place.
fn new_item(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, base: &str, folder: bool) {
    let Some(drive_id) = s.current_drive_id() else {
        return;
    };
    let taken: Vec<String> = s.entries.iter().map(|e| e.name.to_lowercase()).collect();
    let name = fileops::new_name(base, &|n: &str| taken.contains(&n.to_lowercase()));
    let key = format!("{}{name}{}", s.prefix(), if folder { "/" } else { "" });
    let Some(drive) = open_current(s) else {
        return;
    };
    spawn(info, app, s, Job::Create { drive, drive_id, key });
}

/// Ctrl+Z: the last rename, delete into the trash, move or new item, back.
fn undo(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(op) = s.undo.pop() else {
        return;
    };
    let (drive_id, pairs, remove, trashed) = match op.clone() {
        UndoOp::Rename { drive, from, to } => (drive, vec![(to, from)], None, false),
        UndoOp::Trash { drive, gone } => (drive, gone, None, true),
        UndoOp::Move { drive, pairs } => (drive, pairs, None, false),
        UndoOp::Create { drive, key } => (drive, Vec::new(), Some(key), false),
    };
    let Some(drive) = open_drive(s, &drive_id) else {
        return;
    };
    s.info(format!("{}...", op.label()));
    spawn(
        info,
        app,
        s,
        Job::Undo {
            drive,
            pairs,
            remove,
            trashed,
        },
    );
}

// ==== Properties, preview, metadata ====

/// Alt+Enter / Properties: the selected items, the open folder or the
/// selected drive.
fn show_properties(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    match &s.place {
        Place::ThisPc => match s.selected_drive {
            Some(index) => open_properties(info, app, s, Vec::new(), Some(index)),
            None => s.warn("Select a drive to see its properties."),
        },
        Place::QuickAccess => s.info("Open a pinned folder to see its properties."),
        Place::Folder { prefix, .. } => {
            let mut items: Vec<browse::Entry> =
                s.selected_entries().into_iter().cloned().collect();
            if items.is_empty() {
                let index = s.current_drive();
                if prefix.is_empty() {
                    if let Some(index) = index {
                        open_properties(info, app, s, Vec::new(), Some(index));
                    }
                    return;
                }
                items.push(browse::Entry {
                    key: prefix.clone(),
                    name: key::last_segment(prefix).to_string(),
                    is_folder: true,
                    size: None,
                    modified: None,
                    etag: None,
                    known: false,
                });
            }
            open_properties(info, app, s, items, None);
        }
    }
}

/// Opens the Properties dialog and asks for what it needs: a folder's
/// size (one recursive listing), a file's metadata (one HEAD).
pub(crate) fn open_properties(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    items: Vec<browse::Entry>,
    drive: Option<usize>,
) {
    s.popups_opened += 1;
    let serial = s.popups_opened;
    s.popup = Some(Popup::Properties(PropertiesState {
        items: items.clone(),
        drive,
        tab: 0,
        size: None,
        serial,
        metadata: None,
    }));
    println!("AZDRIVE_DONE properties {}", items.len());
    match items.as_slice() {
        [one] if one.is_folder => {
            if let Some(drive) = open_current(s) {
                spawn(
                    info,
                    app,
                    s,
                    Job::Measure {
                        drive,
                        prefix: one.key.clone(),
                        serial,
                    },
                );
            }
        }
        [one] => {
            if let Some(drive) = open_current(s) {
                spawn(
                    info,
                    app,
                    s,
                    Job::Metadata {
                        drive,
                        key: one.key.clone(),
                    },
                );
            }
        }
        _ => {}
    }
}

/// The selection changed: the preview pane's file (fetched on a thread),
/// and the details pane's metadata of a cloud file.
pub(crate) fn request_preview(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(entry) = s.single_selected().cloned() else {
        s.clear_preview();
        return;
    };
    if s.preview.as_ref().map(|p| p.key.as_str()) != Some(entry.key.as_str()) {
        // Another file: the last one's sound stops.
        s.audio = None;
    }
    let local = s
        .current_drive_id()
        .is_some_and(|id| s.is_local_drive(&id));
    if s.settings.details_pane && entry.is_folder && local {
        // The details pane says how many items the folder holds: one cheap count.
        if let Some(index) = s.current_drive() {
            crate::request_counts(info, app, s, index, vec![entry.key.clone()]);
        }
    }
    if s.settings.details_pane && !entry.is_folder && !local && !s.metadata.contains_key(&entry.key)
    {
        if let Some(drive) = open_current(s) {
            spawn(
                info,
                app,
                s,
                Job::Metadata {
                    drive,
                    key: entry.key.clone(),
                },
            );
        }
    }
    if !s.settings.preview_pane {
        return;
    }
    if s.preview.as_ref().is_some_and(|p| p.key == entry.key) {
        return;
    }
    if entry.is_folder {
        s.preview = Some(PreviewState {
            key: entry.key.clone(),
            content: Some(PreviewContent::Message(String::from(
                "A folder: open it to see what it holds.",
            ))),
        });
        return;
    }
    let kind = preview::preview_kind(&entry.name);
    let playable = preview::is_playable_audio(&entry.name);
    if let Some(reason) = preview::no_preview_reason(kind).filter(|_| !playable) {
        println!("AZDRIVE_PREVIEW none {}", entry.key);
        s.preview = Some(PreviewState {
            key: entry.key.clone(),
            content: Some(PreviewContent::Message(reason.to_string())),
        });
        return;
    }
    let Some(drive) = open_current(s) else {
        return;
    };
    s.preview = Some(PreviewState {
        key: entry.key.clone(),
        content: None,
    });
    let temp_dir = s.open_dir.join("preview");
    spawn(
        info,
        app,
        s,
        Job::Preview {
            drive,
            key: entry.key,
            size: entry.size,
            kind,
            temp_dir,
        },
    );
}

// ==== Share ====

/// Share > Copy link: on a cloud drive a link to each selected file that anyone holding it can
/// download for seven days - an S3 presigned URL, signed here with the drive's keys (nothing is
/// sent); a folder has no such link, its address goes along. On a drive on this computer, the
/// items' paths.
fn share_link(info: &mut CallbackInfo, s: &mut DriveState) {
    /// How long a copied link lets anyone download its file (S3's longest).
    const LINK_SECS: u64 = 7 * 24 * 3600;
    let Some(index) = s.current_drive() else {
        return;
    };
    let Some(config) = s.slots[index].entry.s3_config() else {
        copy_path(info, s);
        s.info("Copied the items' paths (a link to share is a cloud drive's).");
        return;
    };
    let Some(credentials) = s.slots[index].credentials.clone() else {
        s.error("The drive's keys are not read yet: open one of its folders first.");
        return;
    };
    let drive = match S3Drive::new(
        config,
        credentials,
        Box::new(AzulTransport::new(crate::USER_AGENT)),
    ) {
        Ok(drive) => drive,
        Err(e) => {
            s.error(format!("No link can be made: {e}"));
            return;
        }
    };
    let drive_id = s.slots[index].entry.id.clone();
    let made: Result<(Vec<String>, usize), String> = {
        let mut links = Vec::new();
        let mut files = 0;
        let mut failed = None;
        for entry in s.selected_entries() {
            if entry.is_folder {
                links.push(item_location(s, &drive_id, &entry.key));
                continue;
            }
            match drive.presigned_get_url(&entry.key, LINK_SECS) {
                Ok(url) => {
                    links.push(url);
                    files += 1;
                }
                Err(e) => {
                    failed = Some(format!("No link to \"{}\": {e}", entry.name));
                    break;
                }
            }
        }
        match failed {
            Some(why) => Err(why),
            None => Ok((links, files)),
        }
    };
    let (links, files) = match made {
        Ok(made) => made,
        Err(why) => {
            s.error(why);
            return;
        }
    };
    info.set_clipboard_content(ClipboardContent {
        plain_text: AzString::from(links.join("\n")),
        styled_runs: StyledTextRunVec::create(),
        html: OptionString::None,
    });
    println!("AZDRIVE_DONE link {files}");
    // A folder has no download link (S3 signs one object per link), so its address went along.
    s.info(match files {
        0 => String::from("A folder has no download link: copied its s3:// address."),
        1 => String::from("Copied a link: anyone holding it can download the file for 7 days."),
        n => format!("Copied {n} links: anyone holding one can download its file for 7 days."),
    });
}

/// Email: a new message in the mail app with the items' addresses.
fn email_selected(s: &mut DriveState) {
    let Some(drive_id) = s.current_drive_id() else {
        return;
    };
    let entries: Vec<browse::Entry> = s.selected_entries().into_iter().cloned().collect();
    let subject = entries
        .iter()
        .map(|e| e.name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    let body = entries
        .iter()
        .map(|e| item_location(s, &drive_id, &e.key))
        .collect::<Vec<_>>()
        .join("\n");
    let url = format!(
        "mailto:?subject={}&body={}",
        azul_storage::sigv4::uri_encode(&subject, true),
        azul_storage::sigv4::uri_encode(&body, true)
    );
    match Url::parse(url.as_str()).into_result() {
        Ok(url) if url.open() => s.info("A new message with the items' addresses is open."),
        _ => s.error("The system has no mail app to open."),
    }
}

/// Zip: the selected items packed into one zip file in the open folder.
fn zip_selected(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    /// Bigger selections are not packed in memory.
    const MAX_ZIP_BYTES: u64 = 256 * 1024 * 1024;
    let items = s.selected_items();
    let known: u64 = items.iter().filter_map(|i| i.size).sum();
    if known > MAX_ZIP_BYTES {
        s.error("The selection is too big to compress in memory (more than 256 MB).");
        return;
    }
    let base = match items.as_slice() {
        [one] => {
            let name = key::last_segment(&one.key);
            let stem = if one.is_folder {
                name
            } else {
                fileops::split_extension(name).0
            };
            format!("{stem}.zip")
        }
        _ => format!("{}.zip", s.place_name()),
    };
    let taken: Vec<String> = s.entries.iter().map(|e| e.name.to_lowercase()).collect();
    let name = fileops::new_name(&base, &|n: &str| taken.contains(&n.to_lowercase()));
    let zip_key = format!("{}{name}", s.prefix());
    let Some(drive) = open_current(s) else {
        return;
    };
    s.info(format!("Compressing {} item(s)...", items.len()));
    spawn(
        info,
        app,
        s,
        Job::Zip {
            drive,
            items,
            zip_key,
        },
    );
}

/// Pin to Quick access (the selected folder, else the open one); a pinned
/// one is unpinned.
fn toggle_pin(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(drive) = s.current_drive_id() else {
        return;
    };
    let (prefix, name) = match s.single_selected() {
        Some(e) if e.is_folder => (e.key.clone(), e.name.clone()),
        _ => (s.prefix().to_string(), s.place_name()),
    };
    if s.settings.is_pinned(&drive, &prefix) {
        s.settings
            .pinned
            .retain(|p| !(p.drive == drive && p.prefix == prefix));
        s.info(format!("\"{name}\" left Quick access."));
    } else {
        s.settings.pinned.push(model::Pinned {
            drive,
            prefix,
            name: name.clone(),
        });
        s.success(format!("\"{name}\" is pinned to Quick access."));
    }
    println!("AZDRIVE_DONE pinned {}", s.settings.pinned.len());
    save_settings(info, app, s);
}

// ==== View settings ====

pub(crate) fn set_layout(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    layout: ViewLayout,
) {
    s.settings.layout = layout;
    println!("AZDRIVE_LAYOUT {}", layout.name());
    request_view_work(info, app, s);
    save_settings(info, app, s);
}

/// Whether the view needs every item's size and date: a sort by Size or Date modified, a
/// grouping by them. A scan of a folder on this computer reads names and kinds only.
pub(crate) fn needs_all_stats(s: &DriveState) -> bool {
    listing::sort_needs_stats(s.settings.sort)
        || matches!(s.settings.group_by, GroupBy::Size | GroupBy::Modified)
}

/// What the rows in view are owed, on worker threads: the sizes and dates of the ones a scan
/// read only the names of, the item counts of the folders among them (the Size column of
/// Details, Content), the thumbnails of the pictures among them (the icon layouts). Nothing for
/// the rows out of view. Whether anything was asked for.
pub(crate) fn request_view_work(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) -> bool {
    let Some(index) = s.current_drive() else {
        return false;
    };
    let range = ui_view::items_in_view(s);
    let mut asked = false;
    if let Some(root) = s.local_root(index) {
        let (stat_keys, folder_keys) = {
            let shown = ui_view::shown_order(s);
            let stats =
                listing::stats_wanted(&shown, range.clone(), &s.stats_asked, listing::STAT_MAX);
            let folders: Vec<String> = shown
                .get(range.clone())
                .unwrap_or(&[])
                .iter()
                .filter(|e| e.is_folder)
                .map(|e| e.key.clone())
                .collect();
            (stats, folders)
        };
        if !stat_keys.is_empty() {
            s.stats_asked.extend(stat_keys.iter().cloned());
            let serial = s.list_serial;
            spawn(
                info,
                app,
                s,
                Job::Stat {
                    root,
                    keys: stat_keys,
                    serial,
                },
            );
            asked = true;
        }
        let counts_shown = matches!(s.settings.layout, ViewLayout::Details | ViewLayout::Content);
        if counts_shown && !folder_keys.is_empty() {
            crate::request_counts(info, app, s, index, folder_keys);
        }
    }
    request_thumbnails_in(info, app, s, range) || asked
}

/// A sort by Size or Date modified (a grouping by them) needs every item's size and date: the
/// ones not stat'ed yet are asked for, a job's worth at a time - each answer asks for the next,
/// and the rows are sorted again when the last one is in.
pub(crate) fn request_sort_stats(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(index) = s.current_drive() else {
        return;
    };
    let Some(root) = s.local_root(index) else {
        return;
    };
    let keys = listing::unknown_keys(&s.entries, &s.stats_asked, listing::STAT_MAX);
    if keys.is_empty() {
        return;
    }
    s.stats_asked.extend(keys.iter().cloned());
    let serial = s.list_serial;
    spawn(info, app, s, Job::Stat { root, keys, serial });
}

/// The icon layouts show pictures as thumbnails: the pictures in view (up to 8 MB) that have
/// none yet are fetched, decoded and scaled down on ONE thread, one answer per picture.
fn request_thumbnails_in(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    range: std::ops::Range<usize>,
) -> bool {
    let wanted = matches!(
        s.settings.layout,
        ViewLayout::MediumIcons | ViewLayout::LargeIcons | ViewLayout::ExtraLargeIcons
    );
    if !wanted || s.current_drive().is_none() {
        return false;
    }
    let items: Vec<(String, Option<u64>)> = {
        let shown = ui_view::shown_order(s);
        shown
            .get(range)
            .unwrap_or(&[])
            .iter()
            .filter(|e| {
                !e.is_folder && preview::preview_kind(&e.name) == preview::PreviewKind::Image
            })
            .filter(|e| {
                !s.thumbnails.contains_key(&e.key) && !s.thumbnails_pending.contains(&e.key)
            })
            .map(|e| (e.key.clone(), e.size))
            .collect()
    };
    if items.is_empty() {
        return false;
    }
    let Some(drive) = open_current(s) else {
        return false;
    };
    for (key, _) in &items {
        s.thumbnails_pending.insert(key.clone());
    }
    spawn(
        info,
        app,
        s,
        Job::Thumbnails {
            drive,
            items,
            max_px: 192,
        },
    );
    true
}

/// Sort by `column` (a second click on the same column reverses it), or
/// in the given direction.
pub(crate) fn sort_by(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    column: Column,
    descending: Option<bool>,
) {
    s.settings.sort = match descending {
        Some(descending) => browse::Sort { column, descending },
        None => s.settings.sort.clicked(column),
    };
    browse::sort_entries(&mut s.entries, s.settings.sort);
    // Sizes and dates of a folder on this computer come with the stat of the rows in view: a
    // sort by them asks for the rest, and sorts again when they are in.
    if needs_all_stats(s) {
        request_sort_stats(info, app, s);
    }
    request_view_work(info, app, s);
    println!(
        "AZDRIVE_SORT {} {}",
        s.settings.sort.column.label(),
        if s.settings.sort.descending {
            "desc"
        } else {
            "asc"
        }
    );
    save_settings(info, app, s);
}

/// A View setting on or off. The preview pane and the details pane share the
/// window's right side, as in Explorer: turning one on turns the other off.
fn toggle(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, which: Toggle) {
    let settings = &mut s.settings;
    match which {
        Toggle::NavigationPane => settings.navigation_pane = !settings.navigation_pane,
        Toggle::PreviewPane => {
            settings.preview_pane = !settings.preview_pane;
            if settings.preview_pane {
                settings.details_pane = false;
            }
        }
        Toggle::DetailsPane => {
            settings.details_pane = !settings.details_pane;
            if settings.details_pane {
                settings.preview_pane = false;
            }
        }
        Toggle::ItemCheckboxes => settings.item_checkboxes = !settings.item_checkboxes,
        Toggle::Extensions => settings.show_extensions = !settings.show_extensions,
        Toggle::HiddenItems => settings.show_hidden = !settings.show_hidden,
        Toggle::ConfirmDelete => settings.confirm_delete = !settings.confirm_delete,
    }
    println!(
        "AZDRIVE_PANES {} {} {}",
        s.settings.navigation_pane, s.settings.preview_pane, s.settings.details_pane
    );
    match which {
        Toggle::HiddenItems => {
            let keys = s.visible_keys();
            let order: Vec<&str> = keys.iter().map(String::as_str).collect();
            s.selection.retain(&order);
            // The tree lists its folders again with or without the hidden ones.
            s.tree.loaded.clear();
            let open: Vec<crate::TreeKey> = s.tree.expanded.iter().cloned().collect();
            for node in open {
                crate::start_tree_listing(info, app, s, node);
            }
            // The counts of folders count hidden items only while they show.
            s.counts.clear();
            s.counts_asked.clear();
            request_view_work(info, app, s);
        }
        Toggle::PreviewPane | Toggle::DetailsPane => {
            s.clear_preview();
            request_preview(info, app, s);
        }
        _ => {}
    }
    save_settings(info, app, s);
}

// ==== Menus ====

/// Delete's choices: into the trash, for good, and whether to ask first.
fn delete_items(app: &RefAny, s: &DriveState) -> Vec<MenuItem> {
    vec![
        menu_item(app, "Recycle (to the trash folder)", Action::Delete, false),
        menu_item(app, "Permanently delete", Action::DeletePermanently, false),
        MenuItem::Separator,
        check_item(
            app,
            "Show delete confirmation",
            Action::Toggle(Toggle::ConfirmDelete),
            s.settings.confirm_delete,
        ),
    ]
}

/// New: a folder, a text document, an empty file.
fn new_items(app: &RefAny) -> Vec<MenuItem> {
    vec![
        menu_item(app, "Folder", Action::NewFolder, false),
        MenuItem::Separator,
        menu_item(app, "Text Document", Action::NewTextDocument, false),
        menu_item(app, "Empty file", Action::NewEmptyFile, false),
    ]
}

/// Open's choices.
fn open_items(app: &RefAny) -> Vec<MenuItem> {
    vec![
        menu_item(app, "Open", Action::Open, false),
        menu_item(app, "Download", Action::Download, false),
        menu_item(app, "Properties", Action::Properties, false),
    ]
}

/// Sort by: the default columns and the shown ones, then the direction.
fn sort_items(app: &RefAny, s: &DriveState) -> Vec<MenuItem> {
    let mut items: Vec<MenuItem> = model::ColumnLayout::default()
        .visible()
        .into_iter()
        .chain(s.settings.columns.visible())
        .fold(Vec::<Column>::new(), |mut all, c| {
            if !all.contains(&c) {
                all.push(c);
            }
            all
        })
        .into_iter()
        .map(|c| {
            check_item(
                app,
                c.label(),
                Action::SortBy(c),
                s.settings.sort.column == c,
            )
        })
        .collect();
    items.push(MenuItem::Separator);
    items.push(check_item(
        app,
        "Ascending",
        Action::SortDescending(false),
        !s.settings.sort.descending,
    ));
    items.push(check_item(
        app,
        "Descending",
        Action::SortDescending(true),
        s.settings.sort.descending,
    ));
    items
}

/// Group by: none or one of the groupings.
fn group_items(app: &RefAny, s: &DriveState) -> Vec<MenuItem> {
    GroupBy::ALL
        .iter()
        .map(|g| check_item(app, g.label(), Action::GroupBy(*g), s.settings.group_by == *g))
        .collect()
}

/// The Details layout's columns (Name always stays).
fn column_items(app: &RefAny, s: &DriveState) -> Vec<MenuItem> {
    Column::ALL
        .iter()
        .map(|c| {
            let mut item = check_item(
                app,
                c.label(),
                Action::ToggleColumn(*c),
                s.settings.columns.is_visible(*c),
            );
            if *c == Column::Name {
                if let MenuItem::String(ref mut string) = item {
                    string.menu_item_state = MenuItemState::Greyed;
                }
            }
            item
        })
        .collect()
}

/// The context menu of the selection (or of the folder itself when nothing
/// is selected), as Explorer's right-click and the menu key show it.
pub(crate) fn context_menu(app: &RefAny, s: &DriveState) -> Menu {
    let item = |label: &str, action: Action| {
        let disabled = why_not(s, &action).is_some();
        menu_item(app, label, action, disabled)
    };
    let submenu = |label: &str, children: Vec<MenuItem>| {
        MenuItem::String(StringMenuItem::create(AzString::from(label)).with_children(children))
    };
    let items = if s.current_drive().is_some() && !s.selection.is_empty() {
        let one_folder = s.single_selected().is_some_and(|e| e.is_folder);
        let mut items = vec![
            item("Open", Action::Open),
            item("Download", Action::Download),
            MenuItem::Separator,
            item("Cut", Action::Cut),
            item("Copy", Action::Copy),
        ];
        if one_folder {
            items.push(item("Pin to Quick access", Action::Pin));
        }
        items.extend([
            MenuItem::Separator,
            item("Compress to zip", Action::Zip),
            item("Copy path", Action::CopyPath),
            MenuItem::Separator,
            item("Delete", Action::Delete),
            item("Rename", Action::Rename),
            MenuItem::Separator,
            item("Properties", Action::Properties),
        ]);
        items
    } else {
        let layouts: Vec<MenuItem> = ViewLayout::ALL
            .iter()
            .map(|l| {
                check_item(
                    app,
                    l.label(),
                    Action::SetLayout(*l),
                    s.settings.layout == *l,
                )
            })
            .collect();
        let sorts: Vec<MenuItem> = s
            .settings
            .columns
            .visible()
            .into_iter()
            .map(|c| check_item(app, c.label(), Action::SortBy(c), s.settings.sort.column == c))
            .collect();
        let groups: Vec<MenuItem> = GroupBy::ALL
            .iter()
            .map(|g| check_item(app, g.label(), Action::GroupBy(*g), s.settings.group_by == *g))
            .collect();
        vec![
            submenu("View", layouts),
            submenu("Sort by", sorts),
            submenu("Group by", groups),
            item("Refresh", Action::Refresh),
            MenuItem::Separator,
            item("Paste", Action::Paste),
            item("Undo", Action::Undo),
            MenuItem::Separator,
            submenu(
                "New",
                vec![
                    item("Folder", Action::NewFolder),
                    item("Text Document", Action::NewTextDocument),
                ],
            ),
            MenuItem::Separator,
            item("Properties", Action::Properties),
        ]
    };
    Menu::create(items)
}

/// A popup's close (x, Escape): a conflict question cancels its transfer.
pub(crate) fn close_popup(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    match s.popup {
        Some(Popup::Conflict { id, .. }) => cancel_transfer(info, app, s, id),
        _ => s.popup = None,
    }
}

// ==== Drives ====

/// Opens the "Add drive" form; with `editing`, prefilled from that drive to
/// enter its keys again.
pub(crate) fn open_drive_form(s: &mut DriveState, editing: Option<usize>) {
    let mut form = DriveForm::default();
    let mut editing_id = None;
    if let Some(slot) = editing.and_then(|i| s.slots.get(i)) {
        if let Some(config) = slot.entry.s3_config() {
            form.name = slot.entry.name.clone();
            form.endpoint = config.endpoint;
            form.region = config.region;
            form.bucket = config.bucket;
            form.path_style = config.path_style;
            editing_id = Some(slot.entry.id.clone());
        }
    }
    s.popups_opened += 1;
    s.popup = Some(Popup::AddDrive {
        form,
        editing: editing_id,
        serial: s.popups_opened,
        testing: false,
        tested: None,
        error: String::new(),
    });
}

/// "Test connection": ONE listing call with the form's keys.
pub(crate) fn test_connection(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(Popup::AddDrive {
        form,
        serial,
        testing,
        tested,
        error,
        ..
    }) = s.popup.as_mut()
    else {
        return;
    };
    if *testing {
        return;
    }
    match form.check() {
        Ok((config, credentials)) => {
            *testing = true;
            *tested = None;
            error.clear();
            let serial = *serial;
            spawn(
                info,
                app,
                s,
                Job::Test {
                    serial,
                    config,
                    credentials,
                },
            );
        }
        Err(problem) => *error = problem,
    }
}

/// "Save drive": the entry into the drives file (no secrets), the keys
/// into the keyring.
pub(crate) fn save_drive(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(Popup::AddDrive {
        form,
        editing,
        error,
        ..
    }) = s.popup.as_mut()
    else {
        return;
    };
    let id = editing
        .clone()
        .unwrap_or_else(|| config::new_drive_id(form.name.trim()));
    let (entry, credentials) = match form.entry(&id).and_then(|entry| {
        let (_, credentials) = form.check()?;
        Ok((entry, credentials))
    }) {
        Ok(parts) => parts,
        Err(problem) => {
            *error = problem;
            return;
        }
    };
    let Some(file_path) = s.drives_file.clone() else {
        *error = String::from("There is no configuration folder to save the drive in.");
        return;
    };
    let saved = DrivesFile::load(&file_path).and_then(|mut file| {
        file.add(entry.clone());
        file.save(&file_path)
    });
    if let Err(e) = saved {
        *error = format!("The drive could not be saved: {e}");
        return;
    }
    let secret = credentials.to_keyring_secret();
    let index = match s.slot_index(&id) {
        Some(index) => {
            s.slots[index] = Slot::new(entry);
            index
        }
        None => {
            s.slots.push(Slot::new(entry));
            s.slots.len() - 1
        }
    };
    s.slots[index].credentials = Some(credentials);
    s.popup = None;
    s.selected_drive = Some(index);
    println!("AZDRIVE_ADDED {id}");
    crate::keyring(
        info,
        s,
        KeyringOp::Store {
            drive_id: id.clone(),
        },
        KeyringCall::Store(config::keyring_key(&id), secret),
    );
    go(info, app, s, Place::folder(&id, ""), true);
}

/// "Add a folder as a drive": the OS's folder dialog picked one.
extern "C" fn on_local_drive_picked(
    mut data: RefAny,
    mut info: CallbackInfo,
    result: RefAny,
) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing; // cancelled
    };
    let root = PathBuf::from(path.inner.as_str());
    with_state(&mut data, &mut info, |info, app, s| {
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.display().to_string());
        let id = config::new_drive_id(&name);
        let entry = DriveEntry {
            id: id.clone(),
            name: name.clone(),
            location: DriveLocation::Local {
                root: root.to_string_lossy().into_owned(),
            },
        };
        if let Some(file_path) = s.drives_file.clone() {
            let saved = DrivesFile::load(&file_path).and_then(|mut file| {
                file.add(entry.clone());
                file.save(&file_path)
            });
            if let Err(e) = saved {
                s.error(format!("The drive could not be saved: {e}"));
                return;
            }
        }
        s.slots.push(Slot::new(entry));
        crate::refresh_disks(s);
        println!("AZDRIVE_ADDED {id}");
        s.success(format!("\"{name}\" is a drive now."));
        go(info, app, s, Place::folder(&id, ""), true);
    })
}

/// What "Choose location..."'s folder dialog answers to: the app and whether it moves or copies.
struct TransferPick {
    app: RefAny,
    kind: TransferKind,
}

/// The folder picked for Move to / Copy to: the selected items go there when it is a folder of
/// one of the drives; otherwise AzDrive's own chooser opens with the path and why it is not.
extern "C" fn on_destination_picked(
    mut data: RefAny,
    mut info: CallbackInfo,
    result: RefAny,
) -> Update {
    let Some((mut app, kind)) = data
        .downcast_ref::<TransferPick>()
        .map(|p| (p.app.clone(), p.kind))
    else {
        return Update::DoNothing;
    };
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing; // cancelled
    };
    let path = PathBuf::from(path.inner.as_str());
    with_state(&mut app, &mut info, |info, app, s| {
        let roots: Vec<(String, PathBuf)> = (0..s.slots.len())
            .filter_map(|i| Some((s.slots[i].entry.id.clone(), s.local_root(i)?)))
            .collect();
        match browse::place_of_path(&path, &roots) {
            Some(place) => transfer_selection_to(info, app, s, kind, place),
            None => {
                s.popups_opened += 1;
                s.popup = Some(Popup::ChooseLocation {
                    kind,
                    text: path.display().to_string(),
                    error: String::from(
                        "This folder is on none of AzDrive's drives: add it as a drive first \
                         (This PC > Computer > Add folder as drive), or type a drive's folder.",
                    ),
                });
            }
        }
    })
}

/// "Remove drive" confirmed: out of the drives file, its keys out of the keyring.
pub(crate) fn forget_drive(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(Popup::ConfirmForget { drive_id }) = s.popup.take() else {
        return;
    };
    if let Some(file_path) = s.drives_file.clone() {
        let saved = DrivesFile::load(&file_path).and_then(|mut file| {
            file.remove(&drive_id);
            file.save(&file_path)
        });
        if let Err(e) = saved {
            s.error(format!("The drives file could not be updated: {e}"));
            return;
        }
    }
    let mut needs_keyring = false;
    if let Some(index) = s.slot_index(&drive_id) {
        let name = s.slots[index].entry.name.clone();
        needs_keyring = s.slots[index].entry.needs_keyring();
        s.slots.remove(index);
        s.info(format!("\"{name}\" was removed from AzDrive. Its files stay where they are."));
    }
    s.selected_drive = None;
    s.tree.expanded.retain(|node| node.0 != drive_id);
    s.tree.loaded.retain(|node, _| node.0 != drive_id);
    s.root_counts.remove(&drive_id);
    s.settings.pinned.retain(|p| p.drive != drive_id);
    if needs_keyring {
        crate::keyring(
            info,
            s,
            KeyringOp::Forget,
            KeyringCall::Delete(config::keyring_key(&drive_id)),
        );
    }
    s.history.clear();
    save_settings(info, app, s);
    go(info, app, s, Place::ThisPc, false);
}

/// The "Choose location" dialog's path: the transfer goes there.
pub(crate) fn choose_location_done(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(Popup::ChooseLocation { kind, text, .. }) = s.popup.take() else {
        return;
    };
    match browse::parse_path(&text, &s.drive_names()) {
        Some(place @ Place::Folder { .. }) => transfer_selection_to(info, app, s, kind, place),
        _ => {
            s.popup = Some(Popup::ChooseLocation {
                kind,
                error: format!("\"{}\" is not a folder of a drive.", text.trim()),
                text,
            });
        }
    }
}

/// A drive tile's or pin's selection (This PC, Quick access).
pub(crate) fn select_drive(s: &mut DriveState, index: usize) {
    s.selected_drive = Some(index);
}

/// Whether `key` is the cut clipboard's item (drawn faded, as Explorer does).
pub(crate) fn is_cut(s: &DriveState, item_key: &str) -> bool {
    s.clipboard.as_ref().is_some_and(|c| {
        c.cut && s.current_drive_id().as_deref() == Some(c.drive.as_str())
            && c.items.iter().any(|i| i.key == item_key)
    })
}

