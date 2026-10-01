//! AzContacts' window on azul's S4 `PimShell`: the app-drawn title row
//! (`NoTitle` + `Titlebar`), a toolbar in the ribbon row (search, New,
//! Import, Export, Duplicates, Settings), the navigation pane
//! (`ShellNavigationPane`: All contacts, Favourites, Possible duplicates,
//! the groups with their counts), the list (letter sections, initials
//! avatars, sort by first or last name, the A-Z jump bar) and the reading
//! pane: the contact card, the edit form, the import preview or the merge
//! screen; a status bar with the counts.
//!
//! Every contact is one file, `contacts/<uid>.vcf` (vCard 4.0) in the
//! user's data folder; the files are read when the window opens and
//! written or deleted after each change, always on an azul Thread through
//! azul-storage (appkit::ui::spawn_file_jobs). `--sample` fills an empty
//! folder with the plan's 300 sample contacts; a `.vcf` given on the
//! command line opens the import preview.
//!
//! On stdout, for scripts/azcontacts_e2e.py: `AZCONTACTS_LOADED <n>`,
//! `AZCONTACTS_VIEW <n>` (the list's length after a search or a filter),
//! `AZCONTACTS_SELECTED <uid> <name>`, `AZCONTACTS_SAVED <uid>`,
//! `AZCONTACTS_DELETED <uid>`, `AZCONTACTS_PROBLEMS <text>`,
//! `AZCONTACTS_IMPORT_PREVIEW <rows> <summary>`, `AZCONTACTS_IMPORTED <n>`,
//! `AZCONTACTS_EXPORTED <key>`, `AZCONTACTS_DUPLICATES <n>`,
//! `AZCONTACTS_MERGED <uid>`, `AZCONTACTS_JUMP <letter>`.

use std::path::{Path, PathBuf};

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, ChipOnRemoveCallbackType,
        DropDownOnChoiceChangeCallbackType, SegmentedOnChangeCallbackType,
        ShellNavigationPaneOnEventCallbackType, SwitchOnToggleCallbackType, TextAreaOnTextInputCallbackType,
        TextInputOnTextInputCallbackType,
    },
    dialog::{FileDialog, FileOpenResult},
    dom::{ClipboardContent, DomNodeId, ScrollIntoViewOptions},
    option::{OptionFileTypeList, OptionString},
    prelude::*,
    shells::{
        PimShell, ShellEmptyState, ShellNavigationGroup, ShellNavigationPane, ShellNavigationPaneEvent,
        ShellNavigationPaneEventKind, ShellThemeAccent, ShellThemeScope,
    },
    str::String as AzString,
    vec::{StringVec, StyledTextRunVec},
    widgets::{
        Avatar, AvatarSize, ButtonType, CheckBoxState, Chip, ChipState, DropDown, OnTextInputReturn, Segmented,
        SegmentedState, StatusBar, StatusBarSegment, Switch, SwitchState, TextArea, TextAreaState,
        TextInputState, TextInputValid, TreeViewNode,
    },
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec},
    files::{FileJob, FileOutcome},
    shortcuts::Shortcut,
    ui::{self as kit, AppSection},
};

use crate::book::{self, Filter, SortBy, ALPHABET};
use crate::contact::{Address, Birthday, Contact, Labeled};
use crate::dupes::{self, MergePlan, Pair, Pick};
use crate::sample;
use crate::store::{self, ImportRow, ImportStatus};
use crate::vcard::Version;

// ==== The app's facts ====

pub const SCREENS: [&str; 5] = ["list", "new", "duplicates", "import", "settings"];

pub const SPEC: AppSpec = AppSpec {
    name: "AzContacts",
    binary: "AzContacts",
    summary: "an address book: one vCard file per contact",
    screens: &SCREENS,
    files_help: ".vcf files to import (vCard 3.0 or 4.0)",
};

pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzContacts",
    version: env!("CARGO_PKG_VERSION"),
    summary: "Your contacts as plain vCard files, one per person, with groups, search, import, export \
              and a duplicates finder. Part of the Azlin apps, built with azul.",
    license: "MIT",
    app_folder: "contacts",
};

pub const SHORTCUTS: [Shortcut; 10] = [
    Shortcut::new("Contacts", "Mod+N", "New contact"),
    Shortcut::new("Contacts", "Mod+F", "Search"),
    Shortcut::new("Contacts", "Mod+E", "Edit the selected contact"),
    Shortcut::new("Contacts", "Mod+S", "Save the contact being edited"),
    Shortcut::new("Contacts", "Escape", "Cancel editing"),
    Shortcut::new("Contacts", "Up / Down", "Previous / next contact in the list"),
    Shortcut::new("Contacts", "Mod+I", "Import a vCard file"),
    Shortcut::new("Contacts", "Mod+Shift+E", "Export the list as vCard"),
    Shortcut::new("Contacts", "Mod+D", "Possible duplicates"),
    Shortcut::new("Panes", "F6 / Shift+F6", "Next / previous pane"),
];

const APP_CATEGORIES: [&str; 1] = ["Contacts"];

/// The labels the edit form offers.
pub const PHONE_LABELS: [&str; 6] = ["mobile", "work", "home", "main", "fax", "other"];
pub const EMAIL_LABELS: [&str; 3] = ["home", "work", "other"];
pub const ADDRESS_LABELS: [&str; 3] = ["home", "work", "other"];

const TAG_LOAD: u64 = 1;
const TAG_WRITE: u64 = 2;
const TAG_IMPORT_FILE: u64 = 3;
const TAG_SAMPLE: u64 = 4;

// ==== State ====

/// The edit form: a draft of the contact and the texts that are not fields of it yet.
#[derive(Clone, Debug, PartialEq)]
pub struct Form {
    pub draft: Contact,
    /// The contact as it was (`None` for a new one).
    pub original: Option<Contact>,
    pub birthday_text: String,
    pub new_group: String,
    pub problems: Vec<String>,
    /// Cancel was pressed with changes: ask before discarding.
    pub confirm_discard: bool,
}

impl Form {
    fn new(contact: Option<&Contact>) -> Form {
        let draft = contact.cloned().unwrap_or_default();
        Form {
            birthday_text: draft.birthday.map(|b| b.to_form()).unwrap_or_default(),
            draft,
            original: contact.cloned(),
            new_group: String::new(),
            problems: Vec::new(),
            confirm_discard: false,
        }
    }

    /// Whether the draft differs from what was opened.
    #[must_use]
    pub fn changed(&self) -> bool {
        let base = self.original.clone().unwrap_or_default();
        let birthday = Birthday::parse(&self.birthday_text);
        self.draft != base || birthday != base.birthday || !self.new_group.trim().is_empty()
    }
}

/// The import preview.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportState {
    pub path: String,
    pub rows: Vec<ImportRow>,
    pub problems: Vec<String>,
    pub group: String,
    pub reading: bool,
}

/// The merge screen.
#[derive(Clone, Debug, PartialEq)]
pub struct MergeState {
    pub pairs: Vec<Pair>,
    pub index: usize,
    pub plan: MergePlan,
}

/// What the reading pane shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Reading {
    /// The selected contact's card (or the empty state).
    Card,
    Edit(Form),
    Import(ImportState),
    Merge(MergeState),
}

/// The app's state.
pub struct ContactsApp {
    pub kit: RefAny,
    pub data_root: PathBuf,
    pub sample: bool,
    pub import_files: Vec<PathBuf>,
    pub book: Vec<Contact>,
    pub loaded: bool,
    pub filter: Filter,
    pub query: String,
    pub sort: SortBy,
    /// The selected contact's UID.
    pub selected: Option<String>,
    pub reading: Reading,
    /// The navigation pane's two groups.
    pub nav_open: [bool; 2],
    pub notice: String,
    /// Delete was pressed: ask first.
    pub confirm_delete: bool,
    /// Pairs the user said are not duplicates (by UID).
    pub ignored: Vec<(String, String)>,
    pub export_version: Version,
    /// The screen `--screen` asked for, applied once the contacts are loaded.
    pub start_screen: String,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `uid1|uid2,uid3|uid4` (the settings value) to pairs.
#[must_use]
pub fn parse_ignored(text: &str) -> Vec<(String, String)> {
    text.split(',')
        .filter_map(|p| p.split_once('|'))
        .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
        .filter(|(a, b)| !a.is_empty() && !b.is_empty())
        .collect()
}

/// Pairs to the settings value.
#[must_use]
pub fn write_ignored(pairs: &[(String, String)]) -> String {
    pairs.iter().map(|(a, b)| format!("{a}|{b}")).collect::<Vec<_>>().join(",")
}

impl ContactsApp {
    fn new(kit_ref: RefAny, args: &AppArgs) -> ContactsApp {
        let mut k = kit_ref.clone();
        let (data_root, sort, version, ignored) = match k.downcast_ref::<kit::Kit>() {
            Some(kit) => (
                kit.data_root.clone(),
                if kit.settings.get("sort") == Some("last") { SortBy::Last } else { SortBy::First },
                if kit.settings.get("export") == Some("3.0") { Version::V3 } else { Version::V4 },
                parse_ignored(kit.settings.get("ignored").unwrap_or("")),
            ),
            None => (PathBuf::from("."), SortBy::First, Version::V4, Vec::new()),
        };
        ContactsApp {
            kit: kit_ref,
            data_root,
            sample: args.sample,
            import_files: args.files.clone(),
            book: Vec::new(),
            loaded: false,
            filter: Filter::All,
            query: String::new(),
            sort,
            selected: None,
            reading: Reading::Card,
            nav_open: [true, true],
            notice: String::new(),
            confirm_delete: false,
            ignored,
            export_version: version,
            start_screen: args.screen.clone().unwrap_or_default(),
        }
    }

    /// The list: indices into `book` in display order.
    fn view(&self) -> Vec<usize> {
        book::view(&self.book, self.sort, &self.filter, &self.query)
    }

    fn selected_index(&self) -> Option<usize> {
        let uid = self.selected.as_ref()?;
        self.book.iter().position(|c| &c.uid == uid)
    }

    fn duplicates(&self) -> Vec<Pair> {
        dupes::find_duplicates(&self.book, dupes::THRESHOLD, &self.ignored)
    }

    fn select(&mut self, uid: Option<String>) {
        self.selected = uid;
        self.confirm_delete = false;
        if let Some(i) = self.selected_index() {
            println!("AZCONTACTS_SELECTED {} {}", self.book[i].uid, self.book[i].display_name());
        }
    }
}

/// The app's start: switches, the kit (settings, data root), the window.
pub fn start() {
    let args = match AppArgs::from_env(&SPEC) {
        Ok(a) => a,
        Err(message) => {
            println!("{message}");
            std::process::exit(if message.contains("USAGE") { 0 } else { 2 });
        }
    };
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &APP_CATEGORIES, args.clone());
    if args.screen.as_deref() == Some("settings") {
        kit::open_settings(&kit_ref, None);
    }
    let app = ContactsApp::new(kit_ref.clone(), &args);
    let config = kit::app_config(&kit_ref);
    let window = kit::window_options(&kit_ref, layout, (1100.0, 720.0), (640.0, 420.0), on_window_created);
    App::create(RefAny::new(app), config).run(window);
}
