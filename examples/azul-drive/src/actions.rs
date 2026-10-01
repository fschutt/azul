//! What AzDrive does: every command of the ribbon, the context menus and
//! the keyboard is an [`Action`], run by [`run_action`] on the state. The
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
    menu::{Menu, MenuItem, MenuItemIcon, MenuItemState, StringMenuItem},
    option::{OptionFileTypeList, OptionMenuItemIcon},
    prelude::*,
    str::String as AzString,
    url::Url,
    vec::StyledTextRunVec,
};
use azul_storage::{
    config::{self, DriveEntry, DriveLocation, DrivesFile},
    key, Drive, DriveError, LocalDrive,
};

use crate::{
    browse::{self, Column, DriveForm, Place},
    fileops::{self, ConflictChoice, Plan, SourceItem, TransferKind, TransferReport},
    go,
    jobs::{Job, PreviewContent},
    keys::{self, Command, Key, Mods, Step},
    model::{self, GroupBy, ViewLayout},
    open_current, open_drive, open_slot, place_up,
    preview::{self, PreviewKind},
    refresh, save_settings, spawn, with_state, ClipboardItems, DriveState, KeyringCall,
    KeyringOp, Popup, PreviewState, PropertiesState, Renaming, Slot, TransferJob, UndoOp,
    HOME_ID,
};

// ==== Actions ====

/// A setting the ribbon turns on and off.
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

/// Every command of the ribbon, the context menus and the backstage.
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
}

/// A ribbon button's / menu item's click data.
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

/// A ribbon button, a menu item or a backstage button was clicked.
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

/// Why `action` cannot run now, or `None` when it can (the ribbon greys the
/// button and says why).
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
            Some(i) if s.slots.get(i).is_some_and(|slot| slot.entry.id == HOME_ID) => {
                Some(String::from("The Home drive stays."))
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
            s.popups_opened += 1;
            s.popup = Some(Popup::ChooseLocation {
                kind,
                text: browse::path_text(&s.place, Some(&s.drive_name(&s.place))),
                error: String::new(),
            });
        }
        Action::DeleteMenu => {
            let items = vec![
                menu_item(app, "Recycle (to the trash folder)", Action::Delete, false),
                menu_item(app, "Permanently delete", Action::DeletePermanently, false),
                MenuItem::Separator,
                check_item(
                    app,
                    "Show delete confirmation",
                    Action::Toggle(Toggle::ConfirmDelete),
                    s.settings.confirm_delete,
                ),
            ];
            info.open_menu_for_hit_node(Menu::create(items));
        }
        Action::Delete => delete_selected(info, app, s, false),
        Action::DeletePermanently => delete_selected(info, app, s, true),
        Action::Rename => start_rename(s),
        Action::NewFolder => new_item(info, app, s, "New folder", true),
        Action::NewItemMenu => {
            let items = vec![
                menu_item(app, "Folder", Action::NewFolder, false),
                MenuItem::Separator,
                menu_item(app, "Text Document", Action::NewTextDocument, false),
                menu_item(app, "Empty file", Action::NewEmptyFile, false),
            ];
            info.open_menu_for_hit_node(Menu::create(items));
        }
        Action::NewTextDocument => new_item(info, app, s, "New Text Document.txt", false),
        Action::NewEmptyFile => new_item(info, app, s, "New file", false),
        Action::Properties => show_properties(info, app, s),
        Action::Open | Action::Edit => open_selected(info, app, s),
        Action::OpenMenu => {
            let items = vec![
                menu_item(app, "Open", Action::Open, false),
                menu_item(app, "Download", Action::Download, false),
                menu_item(app, "Properties", Action::Properties, false),
            ];
            info.open_menu_for_hit_node(Menu::create(items));
        }
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
        Action::SortMenu => {
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
            info.open_menu_for_hit_node(Menu::create(items));
        }
        Action::SortBy(column) => sort_by(info, app, s, column, None),
        Action::SortDescending(descending) => {
            let column = s.settings.sort.column;
            sort_by(info, app, s, column, Some(descending));
        }
        Action::GroupMenu => {
            let items: Vec<MenuItem> = GroupBy::ALL
                .iter()
                .map(|g| check_item(app, g.label(), Action::GroupBy(*g), s.settings.group_by == *g))
                .collect();
            info.open_menu_for_hit_node(Menu::create(items));
        }
        Action::GroupBy(group) => {
            s.settings.group_by = group;
            println!("AZDRIVE_GROUP {}", group.label());
            save_settings(info, app, s);
        }
        Action::ColumnsMenu => {
            let items: Vec<MenuItem> = Column::ALL
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
                .collect();
            info.open_menu_for_hit_node(Menu::create(items));
        }
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
            s.backstage = Some(0);
            s.settings_category = 0;
        }
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
                info.open_menu_for_hit_node(Menu::create(items));
            }
        }
        Action::CloseBackstage => s.backstage = None,
        Action::CloseWindow => info.close_window(),
        Action::ShowTransfers => {
            s.popups_opened += 1;
            s.popup = Some(Popup::Transfers);
        }
    }
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
        ctrl: m.ctrl || m.meta,
        alt: m.alt,
    };
    if in_text_field(&info) {
        return Update::DoNothing;
    }
    let Some(command) = keys::command_for(key_of(code), mods) else {
        return Update::DoNothing;
    };
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    if s.popup.is_some() || s.backstage.is_some() {
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

/// The window was resized: the grid's rows for the arrow keys.
pub(crate) extern "C" fn on_resized(mut data: RefAny, info: CallbackInfo) -> Update {
    let width = info.get_current_window_state().size.dimensions.width;
    if let Some(mut s) = data.downcast_mut::<DriveState>() {
        s.window_width = width;
    }
    Update::DoNothing
}

/// Milliseconds since 1970 (type-ahead's clock).
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Seconds since 1970.
pub(crate) fn now_secs() -> u64 {
    now_ms() / 1000
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
    }
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
            let keys = s.visible_keys();
            let order: Vec<&str> = keys.iter().map(String::as_str).collect();
            s.selection
                .step(&order, step.delta(columns, 10), extend, keep);
            s.print_selection();
            request_preview(info, app, s);
        }
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
