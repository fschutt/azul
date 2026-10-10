//! The navigation pane as Finder's source list (Leopard's, in AzNews' manner), on a soft
//! vertical gradient ([`crate::look`]):
//!
//! - FAVORITES: Quick access (the pinned folders and the places visited last), the standard
//!   folders the Home drive holds (Desktop, Documents, Downloads, Pictures, Music, Videos /
//!   Movies), then the folders the user pinned;
//! - LOCATIONS: This PC, Home, the Azlin data tree and the folders added as drives - a drive
//!   opens its folders under it (its triangle; listed lazily, one listing per opening), a
//!   folder the user added has Finder's eject button, which forgets it;
//! - CLOUD: the S3 drives, each with its state (syncing - the count of its transfers as a pill
//!   -, its keys still in the keyring, connected) and its eject button, then "Add drive";
//! - under the list the ACTIVITY area while transfers run, wait or failed (a click opens the
//!   transfers), and the + (Add drive, Add a folder, Pin) and actions (Options) buttons.
//!
//! A section title has a disclosure triangle (a click opens or closes the section); the row of
//! the place the window shows has the rounded highlight; a folder's row takes dropped items (a
//! move within a drive, Ctrl a copy, a copy across drives), and FAVORITES (its title, Quick
//! access) pins dropped folders. A right click is the row's menu (Open, Properties, Remove from
//! Favorites, Remove drive).
//!
//! The KEYBOARD ([`on_sidebar_key`], from the window's key handler, for the rows, the pane F6
//! lands on and the buttons under the list): Up / Down / Home / End / Page Up / Page Down move
//! between the rows, Right opens a section or a drive (again: steps into it), Left closes it
//! (again: up to its title or its drive), Enter / Space open the row (a section opens or
//! closes), a letter jumps to the next row starting with it, the menu key opens the row's menu.
//! The rows are ONE Tab stop (the selected row, else the first): the arrows move the stop.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use azul::{
    callbacks::CallbackType,
    dom::{DomNodeId, FocusTarget, TabIndex},
    menu::{Menu, MenuItem, StringMenuItem},
    prelude::*,
    str::String as AzString,
};
use azul_appkit::{
    l10n::{self, t, t_args, t_label, Arg, Phrase, Text},
    pieces::{block, text},
};
use azul_storage::key;

use crate::{
    actions::{self, action_ref, menu_item, on_action, Action},
    browse::Place,
    go, ids,
    keys::{Key, Mods},
    look,
    model::{self, Pinned},
    save_settings, start_tree_listing, with_state, DriveState, Popup, TreeKey, TreeState,
    HOME_ID,
};

// ==== The rows, as data ====

/// A section of the source list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Section {
    Favorites,
    Locations,
    Cloud,
}

impl Section {
    /// The title (shown in capitals; a key of the resources).
    #[must_use]
    pub(crate) fn title(self) -> &'static str {
        match self {
            Section::Favorites => "azdrive-side-favorites",
            Section::Locations => "azdrive-side-locations",
            Section::Cloud => "azdrive-side-cloud",
        }
    }

    fn id(self) -> AzString {
        match self {
            Section::Favorites => ids::SIDE_FAVORITES,
            Section::Locations => ids::SIDE_LOCATIONS,
            Section::Cloud => ids::SIDE_CLOUD,
        }
    }
}

impl TreeState {
    /// Whether `section` is open.
    #[must_use]
    pub(crate) fn section_open(&self, section: Section) -> bool {
        match section {
            Section::Favorites => self.favorites_open,
            Section::Locations => self.locations_open,
            Section::Cloud => self.cloud_open,
        }
    }

    pub(crate) fn set_section_open(&mut self, section: Section, open: bool) {
        match section {
            Section::Favorites => self.favorites_open = open,
            Section::Locations => self.locations_open = open,
            Section::Cloud => self.cloud_open = open,
        }
    }
}

/// A standard folder of the Home drive that FAVORITES shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Favorite {
    pub name: String,
    /// Its key in the Home drive: `Documents/`.
    pub prefix: String,
    pub icon: &'static str,
}

/// Finder's standard folders, in its order: (the folder, its icon). Their rows say them in the
/// window's language ([`standard_label`]); the folders keep their names.
const STANDARD: [(&str, &str); 7] = [
    ("Desktop", "desktop_windows"),
    ("Documents", "description"),
    ("Downloads", "download"),
    ("Pictures", "image"),
    ("Music", "music_note"),
    ("Videos", "movie"),
    ("Movies", "movie"),
];

/// A standard folder's row label: Finder's name for it in the window's language (a key), any
/// other folder's name as it is.
fn standard_label(name: &str) -> &str {
    match name {
        "Desktop" => "azdrive-standard-desktop",
        "Documents" => "azdrive-standard-documents",
        "Downloads" => "azdrive-standard-downloads",
        "Pictures" => "azdrive-standard-pictures",
        "Music" => "azdrive-standard-music",
        "Videos" => "azdrive-standard-videos",
        "Movies" => "azdrive-standard-movies",
        other => other,
    }
}

/// The standard folders the folder `home` (the Home drive's) holds, in Finder's order.
#[must_use]
pub(crate) fn standard_folders(home: &Path) -> Vec<Favorite> {
    STANDARD
        .iter()
        .filter(|(name, _)| home.join(name).is_dir())
        .map(|&(name, icon)| Favorite {
            name: name.to_string(),
            prefix: format!("{name}/"),
            icon,
        })
        .collect()
}

/// What a row of the source list is.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RowKind {
    /// A section's title: a click opens or closes the section.
    Section(Section),
    /// A place to go to.
    Go(Place),
    /// Cloud's "Add drive".
    AddDrive,
}

/// One row as the list shows it (and the keyboard walks it).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Row {
    pub kind: RowKind,
    pub label: String,
    pub icon: &'static str,
    /// Folders deep under its drive (0 for a section's own rows and the titles).
    pub depth: usize,
    /// Its DOM id, when the scripts name it.
    pub id: Option<String>,
    /// A section, a drive or a folder that opens: open (`Some(true)`) or closed; `None` for a
    /// row that does not open (a folder listed without folders under it).
    pub open: Option<bool>,
    /// The drive the row is (its slot), for its state and its eject button.
    pub drive: Option<usize>,
    /// A pinned folder's place among the pins (its menu unpins it).
    pub pin: Option<usize>,
}

impl Row {
    fn new(kind: RowKind, label: &str, icon: &'static str) -> Row {
        Row {
            kind,
            label: label.to_string(),
            icon,
            depth: 0,
            id: None,
            open: None,
            drive: None,
            pin: None,
        }
    }

    fn with_id(mut self, id: AzString) -> Row {
        self.id = Some(id.as_str().to_string());
        self
    }

    /// The level the keyboard's Left climbs from: a title 0, a section's row 1, a folder deeper.
    fn level(&self) -> usize {
        match self.kind {
            RowKind::Section(_) => 0,
            _ => self.depth + 1,
        }
    }

    /// The drive and folder a drive's or a folder's row opens (its tree node).
    fn node(&self) -> Option<TreeKey> {
        match &self.kind {
            RowKind::Go(Place::Folder { drive, prefix }) if self.open.is_some() => {
                Some((drive.clone(), prefix.clone()))
            }
            _ => None,
        }
    }
}

/// A drive as the source list knows it.
pub(crate) struct DriveRow<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub icon: &'static str,
    pub local: bool,
}

/// What the source list is made of ([`sources`] reads it off the state; the tests build it).
pub(crate) struct Sources<'a> {
    /// FAVORITES, LOCATIONS, CLOUD: open or closed.
    pub open: [bool; 3],
    /// The standard folders the Home drive holds.
    pub standard: &'a [Favorite],
    pub pins: &'a [Pinned],
    /// Every drive, in the slots' order (a row's `drive` is its index here).
    pub drives: Vec<DriveRow<'a>>,
    /// The drives and folders whose folders are open.
    pub expanded: &'a HashSet<TreeKey>,
    /// The folders listed under a drive or a folder (one listing per opening).
    pub loaded: &'a HashMap<TreeKey, Vec<String>>,
}

/// The source list of the state `s`.
fn sources(s: &DriveState) -> Sources<'_> {
    Sources {
        open: [
            s.tree.favorites_open,
            s.tree.locations_open,
            s.tree.cloud_open,
        ],
        standard: &s.standard_folders,
        pins: &s.settings.pinned,
        drives: s
            .slots
            .iter()
            .map(|slot| DriveRow {
                id: &slot.entry.id,
                name: &slot.entry.name,
                icon: slot.icon(),
                local: slot.is_local(),
            })
            .collect(),
        expanded: &s.tree.expanded,
        loaded: &s.tree.loaded,
    }
}

/// How deep the folders of a drive open in the list at most.
const MAX_DEPTH: usize = 24;

/// Whether the folders `node` lists open (`Some(open)`), or none are under it (`None`: listed,
/// and empty).
fn opens(src: &Sources, node: &TreeKey) -> Option<bool> {
    match src.loaded.get(node) {
        Some(folders) if folders.is_empty() => None,
        _ => Some(src.expanded.contains(node)),
    }
}

/// The folders listed under `node`, a level each, as far as they are open.
fn folder_rows(src: &Sources, node: &TreeKey, depth: usize, out: &mut Vec<Row>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Some(folders) = src.loaded.get(node) else {
        return;
    };
    for folder in folders {
        let child = (node.0.clone(), folder.clone());
        let open = opens(src, &child);
        let mut row = Row::new(
            RowKind::Go(Place::folder(&node.0, folder)),
            key::last_segment(folder),
            "folder",
        );
        row.depth = depth;
        row.open = open;
        out.push(row);
        if open == Some(true) {
            folder_rows(src, &child, depth + 1, out);
        }
    }
}

/// A drive's row, and its folders under it when it is open.
fn drive_rows(src: &Sources, index: usize, drive: &DriveRow, out: &mut Vec<Row>) {
    let node = (drive.id.to_string(), String::new());
    let open = opens(src, &node);
    let mut row = Row::new(
        RowKind::Go(Place::folder(drive.id, "")),
        drive.name,
        drive.icon,
    )
    .with_id(ids::side_drive(drive.id));
    row.open = open;
    row.drive = Some(index);
    out.push(row);
    if open == Some(true) {
        folder_rows(src, &node, 1, out);
    }
}

/// A section's title row.
fn section_row(section: Section, open: bool) -> Row {
    let mut row = Row::new(RowKind::Section(section), section.title(), "").with_id(section.id());
    row.open = Some(open);
    row
}

/// The rows of the source list, top to bottom: each section's title, and its rows while it is
/// open.
#[must_use]
pub(crate) fn rows(src: &Sources) -> Vec<Row> {
    let mut out = Vec::new();
    out.push(section_row(Section::Favorites, src.open[0]));
    if src.open[0] {
        out.push(
            Row::new(RowKind::Go(Place::QuickAccess), "azdrive-quick-access", "star")
                .with_id(ids::SIDE_QUICK_ACCESS),
        );
        for f in src.standard {
            out.push(
                Row::new(
                    RowKind::Go(Place::folder(HOME_ID, &f.prefix)),
                    standard_label(&f.name),
                    f.icon,
                )
                    .with_id(ids::side_favorite(&f.name)),
            );
        }
        for (i, pin) in src.pins.iter().enumerate() {
            // A pinned standard folder has its row already.
            let standard = pin.drive == HOME_ID && src.standard.iter().any(|f| f.prefix == pin.prefix);
            if standard {
                continue;
            }
            let mut row = Row::new(
                RowKind::Go(Place::folder(&pin.drive, &pin.prefix)),
                &pin.name,
                "folder",
            )
            .with_id(ids::side_pin(i));
            row.pin = Some(i);
            out.push(row);
        }
    }
    out.push(section_row(Section::Locations, src.open[1]));
    if src.open[1] {
        out.push(
            Row::new(RowKind::Go(Place::ThisPc), "azdrive-this-pc", "computer")
                .with_id(ids::SIDE_THIS_PC),
        );
        for (i, drive) in src.drives.iter().enumerate().filter(|(_, d)| d.local) {
            drive_rows(src, i, drive, &mut out);
        }
    }
    out.push(section_row(Section::Cloud, src.open[2]));
    if src.open[2] {
        for (i, drive) in src.drives.iter().enumerate().filter(|(_, d)| !d.local) {
            drive_rows(src, i, drive, &mut out);
        }
        out.push(
            Row::new(RowKind::AddDrive, "azdrive-side-add-drive", "add_link")
                .with_id(ids::SIDE_ADD_DRIVE),
        );
    }
    out
}

/// Whether `row` is the place the window shows.
fn shows(place: &Place, row: &Row) -> bool {
    matches!(&row.kind, RowKind::Go(p) if p == place)
}

// ==== The keyboard, as data ====

/// What a key does in the source list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SideMove {
    /// The keyboard moves to row `n`.
    Focus(usize),
    /// Row `n` (a section, a drive, a folder) opens (`true`) or closes.
    Open(usize, bool),
    /// Row `n` is activated: its place opens, its section opens or closes, "Add drive".
    Activate(usize),
    /// Row `n`'s menu opens.
    Menu(usize),
    /// Nothing happens - and nothing else acts on the key.
    Stay,
}

/// The row `current` climbs to with Left: the nearest row above it on a lower level (a
/// folder's drive or parent folder, a row's section title).
fn parent_of(rows: &[Row], current: usize) -> Option<usize> {
    let level = rows.get(current)?.level();
    (0..current).rev().find(|&i| rows[i].level() < level)
}

/// What `key` does on row `current` of `rows` (`None`: the keyboard is in the list's pane or on
/// a button under it - an arrow enters the list at `stop`). `None` back: the key is not the
/// list's (F5, Backspace, Escape, a letter, Enter on a button ...).
#[must_use]
pub(crate) fn key_move(
    rows: &[Row],
    current: Option<usize>,
    stop: usize,
    key: Key,
    shift: bool,
) -> Option<SideMove> {
    let last = rows.len().checked_sub(1)?;
    let navigation = matches!(
        key,
        Key::Up | Key::Down | Key::Left | Key::Right | Key::Home | Key::End | Key::PageUp
            | Key::PageDown
    );
    let Some(c) = current.filter(|c| *c <= last) else {
        return navigation.then_some(SideMove::Focus(stop.min(last)));
    };
    if shift && navigation {
        // Shift+arrow extends a selection - the list has none, and the content's is not
        // under the keyboard.
        return Some(SideMove::Stay);
    }
    let focus = |to: usize| {
        if to == c {
            SideMove::Stay
        } else {
            SideMove::Focus(to)
        }
    };
    let row = &rows[c];
    let into = rows
        .get(c + 1)
        .filter(|next| next.level() > row.level())
        .map(|_| c + 1);
    Some(match key {
        Key::Up => focus(c.saturating_sub(1)),
        Key::Down => focus((c + 1).min(last)),
        Key::Home => focus(0),
        Key::End => focus(last),
        Key::PageUp => focus(c.saturating_sub(10)),
        Key::PageDown => focus((c + 10).min(last)),
        Key::Right => match row.open {
            Some(false) => SideMove::Open(c, true),
            Some(true) => into.map_or(SideMove::Stay, SideMove::Focus),
            None => SideMove::Stay,
        },
        Key::Left => match row.open {
            Some(true) => SideMove::Open(c, false),
            _ => parent_of(rows, c).map_or(SideMove::Stay, SideMove::Focus),
        },
        Key::Enter | Key::Space => SideMove::Activate(c),
        Key::Apps => SideMove::Menu(c),
        Key::F10 if shift => SideMove::Menu(c),
        // The content's selection is not under the keyboard: Delete and F2 do nothing here.
        Key::Delete | Key::F2 => SideMove::Stay,
        _ => return None,
    })
}

// ==== What a row does ====

/// What a row (its click, its triangle, its menu, its eject button) does.
#[derive(Clone, Debug, PartialEq)]
enum SideAction {
    /// The window goes to the place.
    Go(Place),
    /// The section opens or closes.
    Section(Section),
    /// The drive's or the folder's folders open or close (listed on the first opening).
    Expand(TreeKey),
    AddDrive,
    /// The drive is forgotten (after a question): its eject button.
    Eject(String),
    /// Pin `index` leaves FAVORITES.
    Unpin(usize),
    /// The drive's properties.
    Properties(String),
    /// "Encrypt this drive...": the question, then keys, the recovery sheet, the move.
    #[cfg(feature = "encryption")]
    Encrypt(String),
    /// "Unlock with the recovery code...".
    #[cfg(feature = "encryption")]
    Unlock(String),
    /// "I was hacked: new keys...".
    #[cfg(feature = "encryption")]
    Rotate(String),
    /// "Lock down with the recovery code...".
    #[cfg(feature = "encryption")]
    RecoveryLockdown(String),
    /// The folder sync of the drive: pair it with a folder, sync now, pause, stop, open the
    /// synced folder.
    Sync(String, crate::sync_view::SyncAction),
    /// "Restore as of...": an Azlin drive back as it was at a time.
    Restore(String),
    /// "Recover with trusted contacts...".
    #[cfg(feature = "encryption")]
    ContactsRecover(String),
}

/// What a row's callbacks carry.
struct RowRef {
    app: RefAny,
    action: SideAction,
}

fn row_ref(app: &RefAny, action: SideAction) -> RefAny {
    RefAny::new(RowRef {
        app: app.clone(),
        action,
    })
}

fn row_parts(data: &mut RefAny) -> Option<(RefAny, SideAction)> {
    data.downcast_ref::<RowRef>()
        .map(|r| (r.app.clone(), r.action.clone()))
}

/// What a click on `row` does.
fn click_action(row: &Row) -> SideAction {
    match &row.kind {
        RowKind::Section(section) => SideAction::Section(*section),
        RowKind::Go(place) => SideAction::Go(place.clone()),
        RowKind::AddDrive => SideAction::AddDrive,
    }
}

/// The entries of `row`'s menu: (label, what it does).
fn menu_entries(s: &DriveState, row: &Row) -> Vec<(String, SideAction)> {
    let RowKind::Go(place) = &row.kind else {
        return Vec::new();
    };
    let mut entries = vec![(String::from("azdrive-menu-open"), SideAction::Go(place.clone()))];
    if let Some(index) = row.pin {
        entries.push((
            String::from("azdrive-side-unpin"),
            SideAction::Unpin(index),
        ));
    }
    if let Some(slot) = row.drive.and_then(|i| s.slots.get(i)) {
        entries.push((
            String::from("azdrive-menu-properties"),
            SideAction::Properties(slot.entry.id.clone()),
        ));
        #[cfg(feature = "encryption")]
        if slot.entry.azlin().is_some() && crate::encryption::offered() {
            entries.push((
                String::from("azdrive-side-encrypt"),
                SideAction::Encrypt(slot.entry.id.clone()),
            ));
            entries.push((
                String::from("azdrive-side-unlock"),
                SideAction::Unlock(slot.entry.id.clone()),
            ));
            entries.push((
                String::from("azdrive-side-rotate"),
                SideAction::Rotate(slot.entry.id.clone()),
            ));
            entries.push((
                String::from("azdrive-side-lockdown"),
                SideAction::RecoveryLockdown(slot.entry.id.clone()),
            ));
            entries.push((
                String::from("azdrive-side-contacts-recover"),
                SideAction::ContactsRecover(slot.entry.id.clone()),
            ));
        }
        if !slot.is_local() {
            entries.extend(
                crate::sync_view::menu_entries(s, &slot.entry.id)
                    .into_iter()
                    .map(|(label, what)| (label, SideAction::Sync(slot.entry.id.clone(), what))),
            );
        }
        if slot.entry.azlin().is_some() {
            entries.push((
                String::from("azdrive-options-restore"),
                SideAction::Restore(slot.entry.id.clone()),
            ));
        }
        if !slot.is_built_in() {
            entries.push((
                t_args(
                    "azdrive-side-remove-drive",
                    &[("name", Arg::from(slot.entry.name.as_str()))],
                ),
                SideAction::Eject(slot.entry.id.clone()),
            ));
        }
    }
    entries
}

/// `entries` as a menu.
fn menu_of(app: &RefAny, entries: Vec<(String, SideAction)>) -> Menu {
    Menu::create(
        entries
            .into_iter()
            .map(|(label, action)| {
                MenuItem::String(
                    StringMenuItem::create(azul_appkit::l10n::label(&label))
                        .with_callback(row_ref(app, action), on_row_click),
                )
            })
            .collect::<Vec<MenuItem>>(),
    )
}

/// Runs `action` on the state.
fn run(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, action: SideAction) {
    match action {
        SideAction::Go(place) => {
            if place != s.place {
                go(info, app, s, place, true);
            }
        }
        SideAction::Section(section) => {
            let open = s.tree.section_open(section);
            s.tree.set_section_open(section, !open);
        }
        SideAction::Expand(node) => {
            let open = s.tree.expanded.contains(&node);
            set_expanded(info, app, s, node, !open);
        }
        SideAction::AddDrive => actions::run_action(info, app, s, Action::AddDrive),
        SideAction::Eject(drive_id) => {
            if s.popup.is_none() {
                s.popups_opened += 1;
                s.popup = Some(Popup::ConfirmForget { drive_id });
            }
        }
        SideAction::Unpin(index) => {
            if index < s.settings.pinned.len() {
                let pin = s.settings.pinned.remove(index);
                s.info(Phrase::new("azdrive-side-unpinned").arg("name", pin.name.as_str()));
                println!("AZDRIVE_DONE pinned {}", s.settings.pinned.len());
                save_settings(info, app, s);
            }
        }
        SideAction::Properties(drive_id) => {
            if let Some(index) = s.slot_index(&drive_id) {
                actions::open_properties(info, app, s, Vec::new(), Some(index));
            }
        }
        #[cfg(feature = "encryption")]
        SideAction::Encrypt(drive_id) => crate::encryption::ask_encrypt(s, &drive_id),
        #[cfg(feature = "encryption")]
        SideAction::Unlock(drive_id) => crate::encryption::ask_unlock(s, &drive_id),
        #[cfg(feature = "encryption")]
        SideAction::Rotate(drive_id) => crate::encryption::ask_rotate(s, &drive_id),
        #[cfg(feature = "encryption")]
        SideAction::RecoveryLockdown(drive_id) => {
            crate::encryption::ask_recovery_lockdown(s, &drive_id);
        }
        SideAction::Sync(drive_id, what) => {
            crate::sync_jobs::run_action(info, app, s, Some(drive_id), what);
        }
        SideAction::Restore(drive_id) => crate::restore::open(s, &drive_id),
        #[cfg(feature = "encryption")]
        SideAction::ContactsRecover(drive_id) => {
            crate::recovery_contacts::ask_recover(info, app, s, &drive_id, false);
        }
    }
}

/// Opens (`open`) or closes a drive's or a folder's folders; the first opening lists them.
fn set_expanded(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    node: TreeKey,
    open: bool,
) {
    if open {
        s.tree.expanded.insert(node.clone());
        if !s.tree.loaded.contains_key(&node) {
            start_tree_listing(info, app, s, node);
        }
    } else {
        s.tree.expanded.remove(&node);
    }
}

/// Opens (`open`) or closes the section, drive or folder of `row`.
fn open_row(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, row: &Row, open: bool) {
    match &row.kind {
        RowKind::Section(section) => s.tree.set_section_open(*section, open),
        _ => {
            if let Some(node) = row.node() {
                set_expanded(info, app, s, node, open);
            }
        }
    }
}

/// The folders dragged in the window, dropped on FAVORITES: pinned.
fn pin_dropped(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some((drive, items)) = s.dragging.take() else {
        return;
    };
    let mut added = 0;
    for item in items.iter().filter(|item| item.is_folder) {
        if s.settings.is_pinned(&drive, &item.key) {
            continue;
        }
        s.settings.pinned.push(Pinned {
            drive: drive.clone(),
            prefix: item.key.clone(),
            name: key::last_segment(&item.key).to_string(),
        });
        added += 1;
    }
    if added == 0 {
        s.warn(Text::key("azdrive-side-drop-folders"));
        return;
    }
    s.success(Phrase::new("azdrive-side-pinned").arg("count", added));
    println!("AZDRIVE_DONE pinned {}", s.settings.pinned.len());
    save_settings(info, app, s);
}

// ==== Callbacks ====

/// A click on a row (or one of its menu's entries).
extern "C" fn on_row_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, action)) = row_parts(&mut data) else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| run(info, app, s, action))
}

/// A click on a row's triangle (or its eject button): only that, not the row's click.
extern "C" fn on_row_part(data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    on_row_click(data, info)
}

/// What a row's right click carries: the app and its menu.
struct MenuRef {
    app: RefAny,
    entries: Vec<(String, SideAction)>,
}

/// A right click on a row: its menu.
extern "C" fn on_row_menu(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((app, entries)) = data
        .downcast_ref::<MenuRef>()
        .map(|m| (m.app.clone(), m.entries.clone()))
    else {
        return Update::DoNothing;
    };
    info.stop_propagation();
    if !entries.is_empty() {
        // At the row; where the pointer is when the row cannot anchor it.
        let menu = menu_of(&app, entries);
        if !info.open_menu_for_hit_node(menu.clone()) {
            info.open_menu(menu);
        }
    }
    Update::DoNothing
}

/// Items dragged over a row that takes them.
extern "C" fn on_row_drag_over(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.accept_drop();
    Update::DoNothing
}

/// Items dropped on a row: into its folder (a move within a drive, Ctrl a copy, a copy across
/// drives); folders dropped on FAVORITES (its title, Quick access) are pinned.
extern "C" fn on_row_drop(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, action)) = row_parts(&mut data) else {
        return Update::DoNothing;
    };
    info.stop_propagation();
    let copy = info.get_key_modifiers().primary_down();
    with_state(&mut app, &mut info, |info, app, s| match action {
        SideAction::Go(place @ Place::Folder { .. }) => actions::drop_on_place(info, app, s, place, copy),
        SideAction::Go(Place::QuickAccess) | SideAction::Section(Section::Favorites) => {
            pin_dropped(info, app, s);
        }
        _ => s.dragging = None,
    })
}

/// The + under the list: Add drive, Add a folder as a drive, Pin the open folder.
extern "C" fn on_add_menu(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(s) = data.downcast_ref::<DriveState>() else {
        return Update::DoNothing;
    };
    let pinned = match &s.place {
        Place::Folder { drive, prefix } => s.settings.is_pinned(drive, prefix),
        _ => false,
    };
    let pin_label = t_args(
        if pinned {
            "azdrive-side-unpin-place"
        } else {
            "azdrive-side-pin-place"
        },
        &[("name", Arg::from(s.place_name()))],
    );
    let items = vec![
        menu_item(&app, "azdrive-side-add-drive", Action::AddDrive, false),
        menu_item(&app, "azdrive-menu-add-local-drive", Action::AddLocalDrive, false),
        MenuItem::separator(),
        menu_item(
            &app,
            &pin_label,
            Action::Pin,
            actions::why_not(&s, &Action::Pin).is_some(),
        ),
    ];
    drop(s);
    actions::open_menu_below(&mut info, items);
    Update::DoNothing
}

// ==== The DOM ====

/// A count as a pill.
fn pill(n: usize, selected: bool) -> Dom {
    block(
        &format!(
            "{} {}",
            look::PILL,
            if selected { look::PILL_SELECTED } else { "" }
        ),
        text(n.to_string()),
    )
}

/// A cloud drive's state: its glyph, what it says, its transfers. A synced drive says its
/// sync's status line ("Up to date", "Syncing 12 files (340 MB)", "Paused", ...).
fn cloud_state(s: &DriveState, index: usize) -> (&'static str, String, usize) {
    let slot = &s.slots[index];
    let id = slot.entry.id.as_str();
    let busy = s
        .transfers
        .values()
        .filter(|t| t.source_id == id || t.target_id == id)
        .count();
    if busy > 0 {
        ("sync", t("azdrive-side-state-busy"), busy)
    } else if let Some((glyph, says)) = crate::sync_view::sidebar_state(s, id) {
        (glyph, says, 0)
    } else if slot.locked() {
        ("lock", t("azdrive-side-state-locked"), 0)
    } else if slot.drive.is_some() {
        ("cloud_done", t("azdrive-side-state-connected"), 0)
    } else {
        ("cloud_queue", t("azdrive-side-state-not-opened"), 0)
    }
}

/// A section's title row.
fn section_dom(app: &RefAny, row: &Row, section: Section, stop: bool) -> Dom {
    let open = row.open == Some(true);
    let mut css = format!("{} {}", look::SECTION_HEAD, look::ROW_FOCUS);
    if section == Section::Favorites {
        css.push(' ');
        css.push_str(look::ROW_DROP);
    }
    let mut dom = Dom::create_div()
        .with_id(section.id())
        .with_class(ids::SIDE_ROW_CLASS)
        .with_class(ids::SIDE_SECTION_CLASS)
        .with_css(css)
        .with_tab_index(if stop {
            TabIndex::Auto
        } else {
            TabIndex::NoKeyboardFocus
        })
        .with_accessibility_name(l10n::label(section.title()))
        .with_child(
            Dom::create_icon(if open { "arrow_drop_down" } else { "arrow_right" })
                .with_css(look::TRIANGLE),
        )
        .with_child(block(look::SECTION_TITLE, text(l10n::label(section.title()))))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            row_ref(app, SideAction::Section(section)),
            on_row_click,
        );
    if section == Section::Favorites {
        dom = dom
            .with_callback(
                EventFilter::Hover(HoverEventFilter::DragOver),
                row_ref(app, SideAction::Section(section)),
                on_row_drag_over,
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::Drop),
                row_ref(app, SideAction::Section(section)),
                on_row_drop,
            );
    }
    dom
}

/// A place's row (or "Add drive"): its triangle (or the room for one), its icon, its name,
/// a cloud drive's state and pill, an added drive's eject button.
fn row_dom(s: &DriveState, app: &RefAny, row: &Row, stop: bool) -> Dom {
    let selected = shows(&s.place, row);
    let takes_drops = matches!(
        &row.kind,
        RowKind::Go(Place::Folder { .. } | Place::QuickAccess)
    );
    let mut css = format!(
        "{} padding-left: {}px; {} {} {}",
        look::ROW,
        4 + row.depth * 14,
        if selected { look::ROW_SELECTED } else { "" },
        look::ROW_FOCUS,
        if takes_drops { look::ROW_DROP } else { "" },
    );
    if matches!(row.kind, RowKind::AddDrive) {
        css.push_str(" cursor: pointer;");
    }
    let mut dom = Dom::create_div()
        .with_class(ids::SIDE_ROW_CLASS)
        .with_css(css)
        .with_tab_index(if stop {
            TabIndex::Auto
        } else {
            TabIndex::NoKeyboardFocus
        })
        .with_accessibility_name(l10n::label(&row.label));
    if let Some(id) = &row.id {
        dom = dom.with_id(AzString::from(id.as_str()));
    }
    if selected {
        dom.add_class(ids::SIDE_SELECTED_CLASS);
    }
    dom.add_child(match (row.open, row.node()) {
        (Some(open), Some(node)) => {
            Dom::create_icon(if open { "arrow_drop_down" } else { "arrow_right" })
                .with_css(look::TRIANGLE)
                .with_accessibility_name(l10n::label(if open {
                    "azdrive-side-collapse"
                } else {
                    "azdrive-side-expand"
                }))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    row_ref(app, SideAction::Expand(node)),
                    on_row_part,
                )
        }
        _ => Dom::create_div().with_css(look::TRIANGLE_ROOM),
    });
    dom.add_child(Dom::create_icon(row.icon).with_css(format!(
        "{} {}",
        look::ROW_ICON,
        if selected { "" } else { look::ICON_TINT }
    )));
    dom.add_child(block(look::CLIP, text(l10n::label(&row.label))));
    if let Some(index) = row.drive.filter(|i| *i < s.slots.len()) {
        let slot = &s.slots[index];
        if !slot.is_local() {
            let (glyph, says, busy) = cloud_state(s, index);
            dom.add_child(
                Dom::create_icon(glyph)
                    .with_id(ids::side_sync_state(&slot.entry.id))
                    .with_css(look::STATE)
                    .with_accessibility_name(says),
            );
            if busy > 0 {
                dom.add_child(pill(busy, selected));
            }
        }
        if !slot.is_built_in() {
            dom.add_child(
                Dom::create_div()
                    .with_id(ids::side_eject(&slot.entry.id))
                    .with_css(format!(
                        "{} {}",
                        look::EJECT,
                        if selected { look::EJECT_SELECTED } else { "" }
                    ))
                    .with_accessibility_name(t_args(
                        "azdrive-side-eject",
                        &[("name", Arg::from(slot.entry.name.as_str()))],
                    ))
                    .with_child(Dom::create_icon("eject"))
                    .with_callback(
                        EventFilter::Hover(HoverEventFilter::Click),
                        row_ref(app, SideAction::Eject(slot.entry.id.clone())),
                        on_row_part,
                    ),
            );
        }
    }
    dom = dom.with_callback(
        EventFilter::Hover(HoverEventFilter::Click),
        row_ref(app, click_action(row)),
        on_row_click,
    );
    let entries = menu_entries(s, row);
    if !entries.is_empty() {
        dom = dom.with_callback(
            EventFilter::Hover(HoverEventFilter::RightMouseUp),
            RefAny::new(MenuRef {
                app: app.clone(),
                entries,
            }),
            on_row_menu,
        );
    }
    if takes_drops {
        dom = dom
            .with_callback(
                EventFilter::Hover(HoverEventFilter::DragOver),
                row_ref(app, click_action(row)),
                on_row_drag_over,
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::Drop),
                row_ref(app, click_action(row)),
                on_row_drop,
            );
    }
    dom
}

/// The activity area: the transfer that runs (its progress), those that wait, those that
/// failed - while there are any. A click opens the transfers.
fn activity(s: &DriveState, app: &RefAny) -> Option<Dom> {
    let failed = s.queue.failed().len();
    if s.queue.is_idle() && failed == 0 {
        return None;
    }
    let line = |content: String| block(look::ACTIVITY_LINE, text(content));
    let mut area = Dom::create_div()
        .with_id(ids::SIDE_ACTIVITY)
        .with_css(look::ACTIVITY)
        .with_accessibility_name(l10n::label("azdrive-transfers-title"))
        .with_child(block(look::ACTIVITY_HEAD, text(l10n::label("azdrive-transfers-title"))));
    if let Some(job) = s.queue.running() {
        let p = &job.progress;
        area.add_child(line(l10n::t_text(&job.label)));
        area.add_child(block(
            "padding: 3px 10px 2px 10px;",
            ProgressBar::create(p.percent()).dom(),
        ));
        area.add_child(line(t_args(
            "azdrive-side-activity-progress",
            &[
                ("done", Arg::from(p.files_done)),
                ("total", Arg::from(p.files_total)),
                ("percent", Arg::from(format!("{:.0}", p.percent()))),
            ],
        )));
    }
    let waiting = s.queue.waiting();
    if waiting > 0 {
        area.add_child(line(t_args(
            "azdrive-queue-waiting",
            &[("count", Arg::from(waiting))],
        )));
    }
    if failed > 0 {
        area.add_child(block(
            look::ACTIVITY_ERROR,
            text(t_args(
                "azdrive-status-transfers-failed",
                &[("count", Arg::from(failed))],
            )),
        ));
    }
    Some(area.with_callback(
        EventFilter::Hover(HoverEventFilter::Click),
        action_ref(app, Action::ShowTransfers),
        on_action,
    ))
}

/// A small button under the list.
fn small_button(icon: &str, label: &str, id: AzString, data: RefAny, callback: CallbackType) -> Dom {
    Dom::create_div()
        .with_id(id)
        .with_css(look::SMALL_BUTTON)
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_name(l10n::label(label))
        .with_child(Dom::create_icon(icon))
        .with_callback(EventFilter::Hover(HoverEventFilter::Click), data, callback)
}

/// The bar under the list: + (Add drive, Add a folder, Pin) and the actions (Options).
fn foot(app: &RefAny) -> Dom {
    Dom::create_div()
        .with_css(look::SIDEBAR_FOOT)
        .with_child(small_button(
            "add",
            "azdrive-side-add",
            ids::SIDE_ADD,
            app.clone(),
            on_add_menu,
        ))
        .with_child(small_button(
            "settings",
            "azdrive-backstage-options",
            ids::SIDE_ACTIONS,
            action_ref(app, Action::Options),
            on_action,
        ))
}

/// The navigation pane: the source list, the activity area, the buttons under them.
pub(crate) fn sidebar(s: &DriveState, app: &RefAny) -> Dom {
    let rows = rows(&sources(s));
    let stop = rows.iter().position(|r| shows(&s.place, r)).unwrap_or(0);
    let mut list = Dom::create_div()
        .with_id(ids::NAV_PANE)
        .with_css(look::SIDEBAR_LIST);
    for (i, row) in rows.iter().enumerate() {
        list.add_child(match &row.kind {
            RowKind::Section(section) => section_dom(app, row, *section, i == stop),
            _ => row_dom(s, app, row, i == stop),
        });
    }
    let mut column = Dom::create_div()
        .with_id(ids::SIDEBAR)
        .with_class(ids::SIDEBAR_CLASS)
        .with_css(look::SIDEBAR)
        .with_accessibility_name(l10n::label("azdrive-side-sources"))
        .with_child(list);
    if let Some(area) = activity(s, app) {
        column.add_child(area);
    }
    // Cash by post: the orders waiting for their letters (or why one ended).
    if let Some(area) = crate::cash::waiting_area(s, app) {
        column.add_child(area);
    }
    column.add_child(foot(app));
    column
}

// ==== The keyboard, on the DOM ====

/// The DOM id of the shell's navigation pane (BrowserShell's tree pane: what F6 focuses).
const TREE_PANE_ID: &str = "shell-tree";

fn has_class(info: &CallbackInfo, node: DomNodeId, class: &AzString) -> bool {
    info.get_node_classes(node)
        .as_slice()
        .iter()
        .any(|c| c.as_str() == class.as_str())
}

/// The node with the keyboard, when it is in the source list - a row (its row then), the pane
/// F6 lands on, a button under the list.
fn focus_in_sidebar(info: &CallbackInfo) -> Option<(DomNodeId, Option<DomNodeId>)> {
    let focused = info.get_focused_node().into_option()?;
    let mut node = focused;
    let mut row = None;
    for _ in 0..16 {
        if row.is_none() && has_class(info, node, &ids::SIDE_ROW_CLASS) {
            row = Some(node);
        }
        if has_class(info, node, &ids::SIDEBAR_CLASS) {
            return Some((focused, row));
        }
        if info
            .get_node_id(node)
            .into_option()
            .is_some_and(|id| id.as_str() == TREE_PANE_ID)
        {
            return Some((focused, row));
        }
        node = info.get_parent(node).into_option()?;
    }
    None
}

/// The rows' nodes in document order (the list's children that are rows).
fn row_nodes(info: &CallbackInfo, dom: DomNodeId) -> Vec<DomNodeId> {
    let list = info.get_node_id_by_id_attribute(dom.dom, ids::NAV_PANE);
    if list.into_raw() == 0 {
        return Vec::new();
    }
    let list = DomNodeId {
        dom: dom.dom,
        node: list,
    };
    let mut out = Vec::new();
    let mut child = info.get_first_child(list).into_option();
    while let Some(node) = child {
        if has_class(info, node, &ids::SIDE_ROW_CLASS) {
            out.push(node);
        }
        child = info.get_next_sibling(node).into_option();
    }
    out
}

/// Row `to` takes the keyboard - and the list's one Tab stop.
fn focus_row(info: &mut CallbackInfo, nodes: &[DomNodeId], to: usize) {
    let Some(target) = nodes.get(to).copied() else {
        return;
    };
    for (i, node) in nodes.iter().enumerate() {
        info.set_tab_index(
            *node,
            if i == to {
                TabIndex::Auto
            } else {
                TabIndex::NoKeyboardFocus
            },
        );
    }
    info.set_focus(FocusTarget::Id(target));
}

/// The source list's keys, from the window's key handler: `Some` when the keyboard is in the
/// list and the key is the list's (the arrows, Enter, Space, a letter, the menu key - and Enter
/// or Space on a button under the list, left to the engine, which turns it into the button's
/// click), `None` leaves the key to the window (F5, Backspace, Ctrl+C, Escape ...).
pub(crate) fn on_sidebar_key(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    key: Key,
    mods: Mods,
) -> Option<Update> {
    if mods.ctrl || mods.alt {
        return None;
    }
    let (focused, row_node) = focus_in_sidebar(info)?;
    let nodes = row_nodes(info, focused);
    if nodes.is_empty() {
        return None;
    }
    let current = row_node.and_then(|r| nodes.iter().position(|n| *n == r));
    let app = data.clone();
    let mut guard = data.downcast_mut::<DriveState>()?;
    let s = &mut *guard;
    if s.popup.is_some() || s.backstage_shown().is_some() || s.renaming.is_some() {
        return None;
    }
    let rows = rows(&sources(s));
    if rows.len() != nodes.len() {
        // The list changed since it was built: the rebuild on its way shows the new one.
        return None;
    }
    let stop = rows.iter().position(|r| shows(&s.place, r)).unwrap_or(0);
    // A letter: the next row whose name starts with what was typed (Finder's type-ahead).
    if let Key::Char(c) = key {
        let query = s.type_ahead.push(c, actions::now_ms());
        let shown: Vec<String> = rows.iter().map(|r| t_label(&r.label)).collect();
        let names: Vec<&str> = shown.iter().map(String::as_str).collect();
        if let Some(to) = model::type_ahead_match(&names, &query, current) {
            focus_row(info, &nodes, to);
        }
        info.prevent_default();
        return Some(Update::DoNothing);
    }
    let Some(decided) = key_move(&rows, current, stop, key, mods.shift) else {
        // Enter / Space on a button under the list (or the pane): the engine's activation clicks
        // the button - the window's Open must not run on the content, nor veto that click.
        return (current.is_none() && matches!(key, Key::Enter | Key::Space))
            .then_some(Update::DoNothing);
    };
    info.prevent_default();
    Some(match decided {
        SideMove::Stay => Update::DoNothing,
        SideMove::Focus(to) => {
            focus_row(info, &nodes, to);
            Update::DoNothing
        }
        SideMove::Open(at, open) => {
            open_row(info, &app, s, &rows[at], open);
            Update::RefreshDom
        }
        SideMove::Activate(at) => {
            run(info, &app, s, click_action(&rows[at]));
            Update::RefreshDom
        }
        SideMove::Menu(at) => {
            let entries = menu_entries(s, &rows[at]);
            if !entries.is_empty() {
                let menu = menu_of(&app, entries);
                if !info.open_menu_for_node(menu.clone(), nodes[at]) {
                    info.open_menu(menu);
                }
            }
            Update::DoNothing
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(drive: &str, prefix: &str, name: &str) -> Pinned {
        Pinned {
            drive: drive.to_string(),
            prefix: prefix.to_string(),
            name: name.to_string(),
        }
    }

    fn standard() -> Vec<Favorite> {
        vec![
            Favorite {
                name: "Documents".into(),
                prefix: "Documents/".into(),
                icon: "description",
            },
            Favorite {
                name: "Pictures".into(),
                prefix: "Pictures/".into(),
                icon: "image",
            },
        ]
    }

    fn drives() -> Vec<DriveRow<'static>> {
        vec![
            DriveRow {
                id: "home",
                name: "Home",
                icon: "home",
                local: true,
            },
            DriveRow {
                id: "azlin",
                name: "Azlin",
                icon: "folder_special",
                local: true,
            },
            DriveRow {
                id: "bucket-1",
                name: "Bucket",
                icon: "cloud",
                local: false,
            },
        ]
    }

    /// The rows' labels as the list shows them, in English.
    fn labels(rows: &[Row]) -> Vec<String> {
        crate::l10n::in_english();
        rows.iter().map(|r| t_label(&r.label)).collect()
    }

    #[test]
    fn the_source_list_reads_favorites_then_locations_then_cloud() {
        let standard = standard();
        let pins = vec![pin("home", "Documents/", "Documents"), pin("bucket-1", "mail/", "mail")];
        let expanded = HashSet::new();
        let loaded = HashMap::new();
        let src = Sources {
            open: [true, true, true],
            standard: &standard,
            pins: &pins,
            drives: drives(),
            expanded: &expanded,
            loaded: &loaded,
        };
        let rows = rows(&src);
        assert_eq!(
            labels(&rows),
            vec![
                "Favorites",
                "Quick access",
                "Documents",
                "Pictures",
                "mail",
                "Locations",
                "This PC",
                "Home",
                "Azlin",
                "Cloud",
                "Bucket",
                "Add drive\u{2026}",
            ],
            "a pinned standard folder shows once"
        );
        assert_eq!(rows[4].pin, Some(1), "the pin keeps its place among the pins");
        assert_eq!(rows[4].id.as_deref(), Some("__azdrive_side_pin_1"));
        assert_eq!(rows[2].id.as_deref(), Some("__azdrive_side_fav_documents"));
        assert_eq!(rows[7].id.as_deref(), Some("__azdrive_side_drive_home"));
        assert_eq!(rows[10].drive, Some(2), "a drive's row names its slot");
        assert_eq!(rows[7].open, Some(false), "a drive opens its folders");
        assert_eq!(rows[0].open, Some(true), "a section's title opens and closes");
        assert_eq!(
            rows[2].kind,
            RowKind::Go(Place::folder("home", "Documents/")),
            "a standard folder is the Home drive's"
        );
    }

    #[test]
    fn a_closed_section_shows_its_title_alone_and_an_open_drive_its_folders() {
        let standard = standard();
        let pins = Vec::new();
        let mut expanded = HashSet::new();
        expanded.insert(("home".to_string(), String::new()));
        expanded.insert(("home".to_string(), "Code/".to_string()));
        let mut loaded = HashMap::new();
        loaded.insert(
            ("home".to_string(), String::new()),
            vec!["Code/".to_string(), "Empty/".to_string()],
        );
        loaded.insert(("home".to_string(), "Code/".to_string()), vec!["Code/src/".to_string()]);
        loaded.insert(("home".to_string(), "Empty/".to_string()), Vec::new());
        let src = Sources {
            open: [false, true, false],
            standard: &standard,
            pins: &pins,
            drives: drives(),
            expanded: &expanded,
            loaded: &loaded,
        };
        let rows = rows(&src);
        assert_eq!(
            labels(&rows),
            vec!["Favorites", "Locations", "This PC", "Home", "Code", "src", "Empty", "Azlin", "Cloud"]
        );
        assert_eq!(rows[0].open, Some(false));
        assert_eq!((rows[4].depth, rows[5].depth), (1, 2));
        assert_eq!(rows[4].open, Some(true));
        assert_eq!(rows[6].open, None, "a folder listed without folders does not open");
        assert_eq!(rows[5].open, Some(false), "a folder not listed yet may open");
        assert_eq!(rows[3].node(), Some(("home".to_string(), String::new())));
    }

    fn walk() -> Vec<Row> {
        let standard = standard();
        let pins = Vec::new();
        let mut expanded = HashSet::new();
        expanded.insert(("home".to_string(), String::new()));
        let mut loaded = HashMap::new();
        loaded.insert(("home".to_string(), String::new()), vec!["Code/".to_string()]);
        let src = Sources {
            open: [true, true, false],
            standard: &standard,
            pins: &pins,
            drives: drives(),
            expanded: &expanded,
            loaded: &loaded,
        };
        rows(&src)
    }

    #[test]
    fn the_arrows_walk_the_rows_and_hold_at_the_ends() {
        let rows = walk();
        // Favorites, Quick access, Documents, Pictures, Locations, This PC, Home, Code, Azlin,
        // Cloud.
        assert_eq!(rows.len(), 10);
        assert_eq!(key_move(&rows, Some(1), 0, Key::Down, false), Some(SideMove::Focus(2)));
        assert_eq!(key_move(&rows, Some(1), 0, Key::Up, false), Some(SideMove::Focus(0)));
        assert_eq!(key_move(&rows, Some(0), 0, Key::Up, false), Some(SideMove::Stay));
        assert_eq!(key_move(&rows, Some(9), 0, Key::Down, false), Some(SideMove::Stay));
        assert_eq!(key_move(&rows, Some(4), 0, Key::Home, false), Some(SideMove::Focus(0)));
        assert_eq!(key_move(&rows, Some(4), 0, Key::End, false), Some(SideMove::Focus(9)));
        assert_eq!(
            key_move(&rows, None, 6, Key::Down, false),
            Some(SideMove::Focus(6)),
            "from the pane (F6), an arrow enters the list at its stop"
        );
        assert_eq!(key_move(&rows, Some(2), 0, Key::Down, true), Some(SideMove::Stay));
    }

    #[test]
    fn right_opens_and_steps_in_left_closes_and_climbs() {
        let rows = walk();
        assert_eq!(
            key_move(&rows, Some(9), 0, Key::Right, false),
            Some(SideMove::Open(9, true)),
            "a closed section opens"
        );
        assert_eq!(
            key_move(&rows, Some(6), 0, Key::Right, false),
            Some(SideMove::Focus(7)),
            "an open drive: into its first folder"
        );
        assert_eq!(
            key_move(&rows, Some(7), 0, Key::Right, false),
            Some(SideMove::Open(7, true)),
            "a folder not listed yet opens"
        );
        assert_eq!(
            key_move(&rows, Some(6), 0, Key::Left, false),
            Some(SideMove::Open(6, false)),
            "an open drive closes"
        );
        assert_eq!(
            key_move(&rows, Some(7), 0, Key::Left, false),
            Some(SideMove::Focus(6)),
            "a closed folder climbs to its drive"
        );
        assert_eq!(
            key_move(&rows, Some(2), 0, Key::Left, false),
            Some(SideMove::Focus(0)),
            "a favourite climbs to its section's title"
        );
        assert_eq!(
            key_move(&rows, Some(0), 0, Key::Left, false),
            Some(SideMove::Open(0, false)),
            "an open section closes"
        );
        assert_eq!(key_move(&rows, Some(3), 0, Key::Enter, false), Some(SideMove::Activate(3)));
        assert_eq!(key_move(&rows, Some(3), 0, Key::Space, false), Some(SideMove::Activate(3)));
        assert_eq!(key_move(&rows, Some(3), 0, Key::Delete, false), Some(SideMove::Stay));
        assert_eq!(key_move(&rows, Some(3), 0, Key::F5, false), None, "F5 is the window's");
        assert_eq!(key_move(&rows, None, 0, Key::Enter, false), None, "Enter on a button clicks it");
    }

    #[test]
    fn a_standard_folder_counts_only_when_the_home_folder_holds_it() {
        let home = std::env::temp_dir().join(format!("azdrive-sidebar-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("Music")).unwrap();
        std::fs::create_dir_all(home.join("Documents")).unwrap();
        std::fs::write(home.join("Desktop"), b"a file, not a folder").unwrap();
        let found = standard_folders(&home);
        let names: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["Documents", "Music"], "in Finder's order");
        assert_eq!(found[0].prefix, "Documents/");
        let _ = std::fs::remove_dir_all(&home);
    }
}
