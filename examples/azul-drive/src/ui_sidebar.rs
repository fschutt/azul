//! The navigation pane as azul's Sidebar widget: Finder's source list in its three sections
//! (the widget's `SidebarSection`s, each with the triangle that opens and closes it):
//!
//! - FAVORITES: Quick access (the pinned folders and the places visited last), the standard
//!   folders the Home drive holds (Desktop, Documents, Downloads, Pictures, Music, Videos /
//!   Movies), then the folders the user pinned;
//! - LOCATIONS: This PC, Home, the Azlin data tree and the folders added as drives - a drive
//!   opens its folders under it (its triangle; listed lazily, one listing per opening);
//! - CLOUD: the S3 drives, each with its state (a glyph at its end: syncing - the count of its
//!   transfers as the row's badge -, its keys still in the keyring, connected), then "Add
//!   drive".
//!
//! The rows are data first ([`rows`], [`sections_of`]: what the tests read) and the widget's
//! after ([`sidebar`]); a row's tag is its place in [`rows`], which the widget hands back with
//! every report ([`on_sidebar`] runs what a click, a triangle, a section's title, the toggle,
//! the help row, a long press, a right click and a drop ran before). The two pies of the sync and
//! the drive's space are the widget's status slot (`ui_pies::statuses`); the rail is the state
//! the sidebar starts in (the settings keep the user's choice).
//!
//! A folder's row takes dropped items (a move within a drive, Ctrl a copy, a copy across
//! drives), and Quick access pins dropped folders. A right click (a long press on a phone: the
//! quick-action sheet) is the row's menu (Open, Properties, Remove from Favorites, Remove drive,
//! the drive's keys and sync).
//!
//! The KEYBOARD is the widget's: the arrows walk the rows, Right opens a row, Left closes it,
//! Enter opens the place.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use azul::{
    callbacks::SidebarOnEventCallbackType,
    menu::{Menu, MenuItem, StringMenuItem},
    prelude::*,
    str::String as AzString,
    widgets::{
        Sidebar, SidebarEvent, SidebarEventKind, SidebarRow, SidebarSection, SidebarState,
        SidebarSurface,
    },
};
use azul_appkit::l10n::{self, t, t_args, Arg, Phrase, Text};
use azul_storage::key;

use crate::{
    actions::{self, Action},
    browse::Place,
    go,
    model::{Pinned, SidebarMode},
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
            open: None,
            drive: None,
            pin: None,
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
                name: slot.label(),
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
    );
    row.open = open;
    row.drive = Some(index);
    out.push(row);
    if open == Some(true) {
        folder_rows(src, &node, 1, out);
    }
}

/// A section's title row.
fn section_row(section: Section, open: bool) -> Row {
    let mut row = Row::new(RowKind::Section(section), section.title(), "");
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
        out.push(Row::new(
            RowKind::Go(Place::QuickAccess),
            "azdrive-quick-access",
            "star",
        ));
        for f in src.standard {
            out.push(Row::new(
                RowKind::Go(Place::folder(HOME_ID, &f.prefix)),
                standard_label(&f.name),
                f.icon,
            ));
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
            );
            row.pin = Some(i);
            out.push(row);
        }
    }
    out.push(section_row(Section::Locations, src.open[1]));
    if src.open[1] {
        out.push(Row::new(
            RowKind::Go(Place::ThisPc),
            "azdrive-this-pc",
            "computer",
        ));
        for (i, drive) in src.drives.iter().enumerate().filter(|(_, d)| d.local) {
            drive_rows(src, i, drive, &mut out);
        }
    }
    out.push(section_row(Section::Cloud, src.open[2]));
    if src.open[2] {
        for (i, drive) in src.drives.iter().enumerate().filter(|(_, d)| !d.local) {
            drive_rows(src, i, drive, &mut out);
        }
        out.push(Row::new(
            RowKind::AddDrive,
            "azdrive-side-add-drive",
            "add_link",
        ));
    }
    out
}

/// Whether `row` is the place the window shows.
fn shows(place: &Place, row: &Row) -> bool {
    matches!(&row.kind, RowKind::Go(p) if p == place)
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
    /// "Unlock": the drive's keys read from the keychain (the system may ask).
    UnlockKeys(String),
    /// "Lock": the drive's keys leave AzDrive's memory until Unlock.
    LockKeys(String),
    /// "Sign in again…": the drive's form again with its provider's sign-in, for a drive whose
    /// sign-in was ended or expired (the drive keeps its place, its pins and its pairing).
    SignInAgain(String),
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
        if slot.entry.needs_keyring() {
            entries.push(if slot.locked() {
                (
                    String::from("azdrive-side-unlock-keys"),
                    SideAction::UnlockKeys(slot.entry.id.clone()),
                )
            } else {
                (
                    String::from("azdrive-side-lock-keys"),
                    SideAction::LockKeys(slot.entry.id.clone()),
                )
            });
        }
        if crate::sign_in::provider_of(&slot.entry).is_some() {
            entries.push((
                String::from("azdrive-side-sign-in-again"),
                SideAction::SignInAgain(slot.entry.id.clone()),
            ));
        }
        #[cfg(feature = "encryption")]
        if slot.entry.azlin().is_some() && crate::encryption::offered() {
            // Only for a drive known not to be encrypted: an encrypted one would refuse it.
            if slot
                .auto
                .as_ref()
                .is_some_and(|auto| auto.state().offers_encrypt())
            {
                entries.push((
                    String::from("azdrive-side-encrypt"),
                    SideAction::Encrypt(slot.entry.id.clone()),
                ));
            }
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
                s.popup.open(Popup::ConfirmForget { drive_id });
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
        SideAction::UnlockKeys(drive_id) => {
            if let Some(index) = s.slot_index(&drive_id) {
                crate::unlock(info, s, index);
            }
        }
        SideAction::LockKeys(drive_id) => crate::unlock::lock(s, &drive_id),
        SideAction::SignInAgain(drive_id) => {
            if let Some(index) = s.slot_index(&drive_id) {
                crate::actions::open_drive_form(s, Some(index));
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

/// The tree nodes above `place` (SPEC 2.7): its drive's and every folder's between the drive and
/// it, outermost first; none for a drive's root or a place that is no folder.
pub(crate) fn nodes_above(place: &Place) -> Vec<TreeKey> {
    let Place::Folder { drive, prefix } = place else {
        return Vec::new();
    };
    let steps: Vec<&str> = prefix.split('/').filter(|step| !step.is_empty()).collect();
    if steps.is_empty() {
        return Vec::new();
    }
    let mut nodes = vec![(drive.clone(), String::new())];
    let mut above = String::new();
    for step in &steps[..steps.len() - 1] {
        above.push_str(step);
        above.push('/');
        nodes.push((drive.clone(), above.clone()));
    }
    nodes
}

/// The tree opens down to the open folder: every node above it opens, listed on its first
/// opening.
pub(crate) fn reveal_place(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    for node in nodes_above(&s.place) {
        if !s.tree.expanded.contains(&node) {
            set_expanded(info, app, s, node, true);
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

// ==== The rows as the Sidebar widget takes them ====

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

/// One row of the widget, as data: what [`widget_row`] makes a `SidebarRow` of.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RowSpec {
    /// The row's name for the events: its place in [`rows`] (the widget hands it back).
    pub tag: usize,
    /// The label: a key of the resources, or a folder's or drive's own name.
    pub label: String,
    pub icon: &'static str,
    /// How deep a folder row is under its drive (0 for the section's own rows).
    pub depth: usize,
    /// The row opens (a drive, a folder): its triangle, open (`true`) or closed.
    pub disclosure: Option<bool>,
    /// The row is the place the window shows.
    pub selected: bool,
    /// The count after the label: a cloud drive's transfers running; empty for none.
    pub badge: String,
    /// A cloud drive's state at the row's end: its glyph and what it says.
    pub status: Option<(&'static str, String)>,
    /// Items dragged in the window can be dropped on the row.
    pub accepts_drops: bool,
    /// The row is a button of the icon rail (the top level's rows: the folders under a drive
    /// stay out of it).
    pub in_rail: bool,
}

/// A section of the widget, as data.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SectionSpec {
    pub section: Section,
    /// Its title, a key of the resources.
    pub title: &'static str,
    /// Whether the section shows its rows (its title's triangle is open).
    pub open: bool,
    pub rows: Vec<RowSpec>,
}

/// What the rows say of the window: the place it shows and each drive's state.
pub(crate) struct Facts<'a> {
    pub place: &'a Place,
    /// By drive (the slot's place in [`Sources::drives`]): a cloud drive's glyph, words and the
    /// number of its transfers running; `None` for a drive on this computer.
    pub cloud: Vec<Option<(&'static str, String, usize)>>,
}

/// The widget's sections of the source list's `rows`: a section's title row starts a section, the
/// rows after it are its rows. A row's tag is its place in `rows`.
#[must_use]
pub(crate) fn sections_of(rows: &[Row], facts: &Facts) -> Vec<SectionSpec> {
    let mut sections: Vec<SectionSpec> = Vec::new();
    for (tag, row) in rows.iter().enumerate() {
        if let RowKind::Section(section) = row.kind {
            sections.push(SectionSpec {
                section,
                title: section.title(),
                open: row.open == Some(true),
                rows: Vec::new(),
            });
            continue;
        }
        let Some(current) = sections.last_mut() else {
            continue;
        };
        let state = row
            .drive
            .and_then(|index| facts.cloud.get(index))
            .and_then(Option::as_ref);
        current.rows.push(RowSpec {
            tag,
            label: row.label.clone(),
            icon: row.icon,
            depth: row.depth,
            disclosure: row.node().and(row.open),
            selected: shows(facts.place, row),
            badge: match state {
                Some((_, _, busy)) if *busy > 0 => busy.to_string(),
                _ => String::new(),
            },
            status: state.map(|(glyph, says, _)| (*glyph, says.clone())),
            accepts_drops: takes_drops(row),
            in_rail: row.depth == 0,
        });
    }
    sections
}

/// Whether items dropped on `row` go somewhere: into a folder, or onto Quick access to pin.
fn takes_drops(row: &Row) -> bool {
    matches!(
        &row.kind,
        RowKind::Go(Place::Folder { .. } | Place::QuickAccess)
    )
}

/// The widget's section of `spec`.
fn widget_section(spec: &SectionSpec) -> SidebarSection {
    let mut section =
        SidebarSection::create(l10n::label(spec.title)).with_collapsible(spec.open);
    for row in &spec.rows {
        section = section.with_row(widget_row(row));
    }
    section
}

/// The widget's row of `spec`.
fn widget_row(spec: &RowSpec) -> SidebarRow {
    let mut row = SidebarRow::create(l10n::label(&spec.label))
        .with_icon(AzString::from(spec.icon))
        .with_tag(spec.tag)
        .with_depth(spec.depth)
        .with_selected(spec.selected)
        .with_in_rail(spec.in_rail)
        .with_accepts_drops(spec.accepts_drops);
    if let Some(open) = spec.disclosure {
        row = row.with_disclosure(open);
    }
    if !spec.badge.is_empty() {
        row = row.with_badge(AzString::from(spec.badge.as_str()));
    }
    if let Some((glyph, says)) = &spec.status {
        row = row.with_status(AzString::from(*glyph), AzString::from(says.as_str()));
    }
    row
}

/// The left sidebar: the source list in its sections, the status pies above its toggle
/// (`ui_pies::statuses`), the help row, the cash orders still waiting for their letters in the
/// slot over the rows. It is the rail until the user opens it.
pub(crate) fn sidebar(s: &DriveState, app: &RefAny, frame_width: f32) -> Sidebar {
    let rows = rows(&sources(s));
    let facts = Facts {
        place: &s.place,
        cloud: s
            .slots
            .iter()
            .enumerate()
            .map(|(index, slot)| (!slot.is_local()).then(|| cloud_state(s, index)))
            .collect(),
    };
    let mut sidebar = Sidebar::create()
        .with_state(s.sidebar_state())
        .with_surface(SidebarSurface::Theme)
        .with_form(s.form)
        .with_label(l10n::label("azdrive-side-sources"))
        .with_help(l10n::label("azdrive-side-help"))
        .with_window_controls(s.controls.0, s.controls.1)
        // With the frame's width the column slides between its states instead of jumping.
        .with_frame_width(frame_width)
        .with_on_event(app.clone(), on_sidebar as SidebarOnEventCallbackType);
    for section in sections_of(&rows, &facts) {
        sidebar = sidebar.with_section(widget_section(&section));
    }
    for status in crate::ui_pies::statuses(s, app) {
        sidebar = sidebar.with_status(status);
    }
    // Cash by post: the orders waiting for their letters (or why one ended).
    if let Some(area) = crate::cash::waiting_area(s, app) {
        sidebar = sidebar.with_header(area);
    }
    sidebar
}

// ==== What the sidebar reports ====

/// The sidebar's state the settings keep for `state`.
#[must_use]
pub(crate) fn mode_of(state: SidebarState) -> SidebarMode {
    match state {
        SidebarState::Expanded => SidebarMode::Expanded,
        SidebarState::Rail => SidebarMode::Rail,
        SidebarState::Handle => SidebarMode::Handle,
    }
}

/// The section a section title of the widget is: the sections come in the order of [`Section`].
#[must_use]
pub(crate) fn section_at(index: usize) -> Option<Section> {
    [Section::Favorites, Section::Locations, Section::Cloud]
        .get(index)
        .copied()
}

/// What the widget's report `event` does to the window: what a click on the row, its triangle,
/// the section's title, the toggle, the help row and a long press did in the source list before.
fn side_event(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, event: SidebarEvent) {
    match event.kind {
        SidebarEventKind::RowClicked => {
            let rows = rows(&sources(s));
            let Some(row) = rows.get(event.tag) else {
                return;
            };
            let action = click_action(row);
            let opens_place = matches!(action, SideAction::Go(_) | SideAction::AddDrive);
            run(info, app, s, action);
            // A phone's drawer closes once a place was chosen.
            if s.phone && opens_place && s.settings.sidebar == SidebarMode::Expanded {
                s.settings.sidebar = SidebarMode::Rail;
                save_settings(info, app, s);
            }
        }
        SidebarEventKind::RowToggled => {
            let rows = rows(&sources(s));
            if let Some(row) = rows.get(event.tag) {
                open_row(info, app, s, row, event.expand);
            }
        }
        SidebarEventKind::SectionToggled => {
            if let Some(section) = section_at(event.index) {
                s.tree.set_section_open(section, event.expand);
            }
        }
        SidebarEventKind::StateChanged => {
            let mode = mode_of(event.state);
            if s.settings.sidebar != mode {
                s.settings.sidebar = mode;
                println!("AZDRIVE_SIDEBAR {mode:?}");
                save_settings(info, app, s);
            }
        }
        SidebarEventKind::HelpClicked => crate::ui_command_bar::show_help(info, app, s),
        SidebarEventKind::RowLongPressed => {
            s.sheet = Some(crate::ui_sheet::Sheet::Place(event.tag));
        }
        SidebarEventKind::RowDropped => {
            let copy = info.get_key_modifiers().primary_down();
            let rows = rows(&sources(s));
            match rows.get(event.tag).map(click_action) {
                Some(SideAction::Go(place @ Place::Folder { .. })) => {
                    actions::drop_on_place(info, app, s, place, copy);
                }
                Some(SideAction::Go(Place::QuickAccess)) => pin_dropped(info, app, s),
                _ => s.dragging = None,
            }
        }
        // The rows carry no pie or collaboration icon and the rail no quick-access button; a
        // right click is answered apart.
        SidebarEventKind::RowStatusClicked
        | SidebarEventKind::RowCollabToggled
        | SidebarEventKind::QuickAction
        | SidebarEventKind::RowMenuRequested => {}
    }
}

/// The sidebar's reports.
extern "C" fn on_sidebar(mut data: RefAny, mut info: CallbackInfo, event: SidebarEvent) -> Update {
    if event.kind == SidebarEventKind::RowMenuRequested {
        return open_row_menu(&mut data, &mut info, event.tag);
    }
    with_state(&mut data, &mut info, |info, app, s| side_event(info, app, s, event))
}

/// A right click on a row: its menu, at the row (where the pointer is when the row cannot
/// anchor it).
fn open_row_menu(data: &mut RefAny, info: &mut CallbackInfo, tag: usize) -> Update {
    let app = data.clone();
    let entries = {
        let Some(s) = data.downcast_ref::<DriveState>() else {
            return Update::DoNothing;
        };
        let rows = rows(&sources(&s));
        rows.get(tag)
            .map(|row| menu_entries(&s, row))
            .unwrap_or_default()
    };
    if entries.is_empty() {
        return Update::DoNothing;
    }
    let menu = menu_of(&app, entries);
    if !info.open_menu_for_hit_node(menu.clone()) {
        info.open_menu(menu);
    }
    Update::DoNothing
}

// ==== The phone's sheet of a row ====

/// The icon of a menu entry in the quick-action sheet.
#[must_use]
fn entry_icon(action: &SideAction) -> &'static str {
    match action {
        SideAction::Go(_) => "folder_open",
        SideAction::Unpin(_) => "push_pin",
        SideAction::Properties(_) => "info",
        SideAction::UnlockKeys(_) => "lock_open",
        SideAction::LockKeys(_) => "lock",
        SideAction::SignInAgain(_) => "login",
        SideAction::Sync(..) => "sync",
        SideAction::Restore(_) => "history",
        SideAction::Eject(_) => "eject",
        _ => "more_horiz",
    }
}

/// One entry of a row's quick-action sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SheetEntry {
    pub label: String,
    pub icon: &'static str,
    /// Removing a drive: shown in red.
    pub danger: bool,
}

/// The quick-action sheet of row `tag` (what a long press opens on a phone): the row's name and
/// the entries of its menu. `None` for a row with no menu (a section, "Add drive").
#[must_use]
pub(crate) fn place_sheet(s: &DriveState, tag: usize) -> Option<(String, Vec<SheetEntry>)> {
    let rows = rows(&sources(s));
    let row = rows.get(tag)?;
    let entries = menu_entries(s, row);
    if entries.is_empty() {
        return None;
    }
    let items = entries
        .iter()
        .map(|(label, action)| SheetEntry {
            label: label.clone(),
            icon: entry_icon(action),
            danger: matches!(action, SideAction::Eject(_)),
        })
        .collect();
    Some((row.label.clone(), items))
}

/// Entry `choice` of row `tag`'s sheet was chosen.
pub(crate) fn run_place_sheet(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    tag: usize,
    choice: usize,
) {
    let rows = rows(&sources(s));
    let Some(row) = rows.get(tag) else {
        return;
    };
    if let Some((_, action)) = menu_entries(s, row).into_iter().nth(choice) {
        run(info, app, s, action);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use azul_appkit::l10n::t_label;

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
    fn the_tree_opens_the_drive_and_every_folder_above_the_open_one() {
        let home = |prefix: &str| ("home".to_string(), prefix.to_string());
        assert_eq!(
            nodes_above(&Place::folder("home", "Documents/Projekte/Calmet 1732/")),
            vec![home(""), home("Documents/"), home("Documents/Projekte/")]
        );
        assert_eq!(nodes_above(&Place::folder("home", "")), Vec::<TreeKey>::new());
        assert_eq!(nodes_above(&Place::folder("home", "Documents/")), vec![home("")]);
        assert_eq!(nodes_above(&Place::ThisPc), Vec::<TreeKey>::new());
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

#[cfg(test)]
#[path = "ui_sidebar_test.rs"]
mod ui_sidebar_test;
