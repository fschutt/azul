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

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, ChipOnRemoveCallbackType,
        DatePickerOnChangeCallbackType, DropDownOnChoiceChangeCallbackType,
        NumberInputOnValueChangeCallbackType, SegmentedOnChangeCallbackType,
        ShellNavigationPaneOnEventCallbackType, SwitchOnToggleCallbackType, TextAreaOnTextInputCallbackType,
        TextInputOnTextInputCallbackType, ToolbarOnEventCallbackType,
    },
    dialog::{FileDialog, FileOpenResult},
    dom::{ClipboardContent, DomNodeId, ScrollIntoViewOptions},
    error::ResultRawImageDecodeImageError,
    image::{ImageRef, RawImage},
    option::{OptionFileTypeList, OptionString},
    prelude::*,
    shells::{
        PimShell, ShellEmptyState, ShellNavigationGroup, ShellNavigationPane, ShellNavigationPaneEvent,
        ShellNavigationPaneEventKind, ShellThemeAccent, ShellThemeScope,
    },
    str::String as AzString,
    vec::{StyledTextRunVec, U8VecRef},
    widgets::{
        Avatar, AvatarSize, ButtonType, CheckBoxState, Chip, ChipState, DatePicker, DatePickerState, DropDown,
        NumberInput, NumberInputState, OnTextInputReturn, Segmented,
        SegmentedState, StatusBar, StatusBarSegment, Switch, SwitchState, TextArea, TextAreaState,
        TextInputState, TextInputValid, Toolbar, ToolbarEvent, ToolbarEventKind, ToolbarItem,
        TreeViewNode,
    },
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec},
    files::{FileJob, FileOutcome},
    l10n::{date_text, label, named, t, t_args, t_text, Arg, DateStyle, Phrase, Text},
    pieces::{block, button, column, flex_row, primary, strs, text},
    settings::AppSettings,
    shortcuts::Shortcut,
    ui::{self as kit, AppSection},
};

use crate::book::{self, Filter, SortBy, ALPHABET};
use crate::contact::{Address, Birthday, Contact, Labeled};
use crate::dupes::{self, MergePlan, Pair, Pick};
use crate::ids;
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
    // The About page says it by `azcontacts-about-summary` (l10n::app_word).
    summary: "Your contacts as plain vCard files, one per person, with groups, search, import, export \
              and a duplicates finder. Part of the Azlin apps, built with azul.",
    license: "MIT",
    app_folder: "contacts",
};

/// The keys (the group and what each does are keys of the resources).
pub const SHORTCUTS: [Shortcut; 9] = [
    Shortcut::new("azcontacts-contacts", "Mod+N", "azcontacts-new-contact"),
    Shortcut::new("azcontacts-contacts", "Mod+E", "azcontacts-shortcut-edit"),
    Shortcut::new("azcontacts-contacts", "Mod+S", "azcontacts-shortcut-save"),
    Shortcut::new("azcontacts-contacts", "Escape", "azcontacts-shortcut-cancel"),
    Shortcut::new("azcontacts-contacts", "Up / Down", "azcontacts-shortcut-up-down"),
    Shortcut::new("azcontacts-contacts", "Mod+I", "azcontacts-shortcut-import"),
    Shortcut::new("azcontacts-contacts", "Mod+Shift+E", "azcontacts-shortcut-export"),
    Shortcut::new("azcontacts-contacts", "Mod+D", "azcontacts-possible-duplicates"),
    Shortcut::new("azcontacts-shortcut-panes", "F6 / Shift+F6", "azcontacts-shortcut-panes-next"),
];

/// The app's category of the settings page (said by `azcontacts-category-contacts`).
const APP_CATEGORIES: [&str; 1] = ["Contacts"];

/// The labels the edit form offers: vCard's words, kept in the files as they are; the window
/// says them by `azcontacts-label-<label>` ([`label_word`]).
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
    /// What the form refuses (said at layout).
    pub problems: Vec<Text>,
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
    /// What the file or the reading said (said at layout).
    pub problems: Vec<Text>,
    pub group: String,
    pub reading: bool,
    /// A CSV file's table and how its columns map to contact fields (`None`: a .vcf).
    pub csv: Option<CsvImport>,
}

/// A CSV import's table and its column mapping (one field per column, `csv::guess`ed from
/// the header, changed in the preview).
#[derive(Clone, Debug, PartialEq)]
pub struct CsvImport {
    pub table: crate::csv::Table,
    pub mapping: Vec<crate::csv::Field>,
}

/// The file is a CSV file (by its name).
fn is_csv(path: &str) -> bool {
    path.trim().to_ascii_lowercase().ends_with(".csv")
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
    /// Decoded photos by `photo::key` (`None`: azul could not decode it - the initials show),
    /// filled after each change for the contact shown and the form (`refresh_photos`).
    pub photos: BTreeMap<u64, Option<ImageRef>>,
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
                sort_of(&kit.settings),
                export_version_of(&kit.settings),
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
            photos: BTreeMap::new(),
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
    let mut config = kit::app_config(&kit_ref);
    crate::l10n::register(&mut config);
    let window = kit::window_options(&kit_ref, layout, (1100.0, 720.0), (640.0, 420.0), on_window_created);
    App::create(RefAny::new(app), config).run(window);
}

// ==== Words ====

/// A contact's name as the window shows it: its display name; a contact without a name, an
/// email or a phone number in the window's language.
fn shown_name(c: &Contact) -> String {
    let name = c.display_name();
    if name == crate::contact::NO_NAME {
        t("azcontacts-no-name")
    } else {
        name
    }
}

/// A birthday in the window's language: "14 March 1987", "14 March" without its year.
fn birthday_text(b: &Birthday) -> String {
    match b.year {
        Some(year) => date_text(DateStyle::Date, year, b.month, b.day, 0),
        None => date_text(DateStyle::DayMonth, 2000, b.month, b.day, 0),
    }
}

/// A row's label (vCard's `mobile`, `work`, or the user's own) in the window's language.
fn label_word(label: &str) -> String {
    named(SPEC.name, "label", label)
}

// ==== Navigation ====

fn navigation(s: &ContactsApp, app: &RefAny) -> Dom {
    let favorites = s.book.iter().filter(|c| c.favorite).count();
    let dupes = s.duplicates().len();
    let duplicates_open = matches!(s.reading, Reading::Merge(_));
    let contacts = TreeViewNode::create(t_args("azcontacts-all-contacts-count", &[("count", Arg::from(s.book.len()))]))
        .with_icon("contacts")
        .with_expanded(true)
        .with_selected(s.filter == Filter::All && !duplicates_open)
        .with_child(
            TreeViewNode::create(t_args("azcontacts-favourites-count", &[("count", Arg::from(favorites))]))
                .with_icon("star")
                .with_selected(s.filter == Filter::Favorites && !duplicates_open),
        )
        .with_child(
            TreeViewNode::create(t_args("azcontacts-duplicates-count", &[("count", Arg::from(dupes))]))
                .with_icon("merge")
                .with_selected(duplicates_open),
        );
    let mut groups = TreeViewNode::create(label("azcontacts-groups")).with_icon("group").with_expanded(true);
    let counts = book::group_counts(&s.book);
    for (name, count) in &counts {
        groups = groups.with_child(
            TreeViewNode::create(format!("{name} ({count})"))
                .with_icon("label")
                .with_selected(s.filter == Filter::Group(name.clone()) && !duplicates_open),
        );
    }
    ShellNavigationPane::create()
        .with_header(primary("azcontacts-new-contact", ids::CONTACTS_NEW, app, on_new))
        .with_group(
            ShellNavigationGroup::create(label("azcontacts-contacts"), contacts)
                .with_count(s.book.len())
                .with_open(s.nav_open[0]),
        )
        .with_group(
            ShellNavigationGroup::create(label("azcontacts-groups"), groups)
                .with_count(counts.len())
                .with_open(s.nav_open[1]),
        )
        .with_label(label("azcontacts-contacts-and-groups"))
        .with_on_event(app.clone(), on_nav as ShellNavigationPaneOnEventCallbackType)
        .dom()
}

// ==== The list ====

struct RowRef {
    app: RefAny,
    uid: String,
}

struct LetterRef {
    letter: char,
}

fn filter_title(s: &ContactsApp) -> String {
    match &s.filter {
        Filter::All => t("azcontacts-all-contacts"),
        Filter::Favorites => t("azcontacts-favourites"),
        Filter::Group(g) => g.clone(),
    }
}

fn contact_row(s: &ContactsApp, app: &RefAny, c: &Contact) -> Dom {
    let selected = s.selected.as_deref() == Some(c.uid.as_str());
    let mut texts = vec![block("font-size: 13px;", text(shown_name(c)))];
    let subtitle = c.subtitle();
    if !subtitle.is_empty() {
        texts.push(block("font-size: 11px; opacity: 0.7;", text(subtitle)));
    }
    let mut children = vec![
        avatar(s, c, AvatarSize::Small),
        column("flex-grow: 1; padding-left: 8px; min-width: 0px;", texts),
    ];
    if c.favorite {
        children.push(block("padding: 0px 6px; font-size: 13px;", text("\u{2605}")));
    }
    flex_row(
        &format!(
            "padding: 4px 8px; cursor: pointer; {}",
            // Under flora a selected row lies on the theme's selection (every widget's list).
            if selected {
                "background-color: rgba(64, 128, 255, 0.18); @theme(flora) { background-color: \
                 system:selection-background; }"
            } else {
                ""
            }
        ),
        children,
    )
    .with_class(ids::CONTACT_ROW_CLASS)
    .with_accessibility_name(shown_name(c))
    .with_callback(
        EventFilter::Hover(HoverEventFilter::MouseUp),
        RefAny::new(RowRef {
            app: app.clone(),
            uid: c.uid.clone(),
        }),
        on_row,
    )
}

fn jump_bar(present: &[char]) -> Dom {
    let mut bar = Dom::create_div()
        .with_id(ids::CONTACTS_JUMP)
        .with_css("display: flex; flex-direction: column; width: 18px; flex-shrink: 0; font-size: 10px; padding-top: 4px;");
    for letter in ALPHABET {
        let has = present.contains(&letter);
        bar.add_child(
            Dom::create_div()
                .with_id(ids::jump(letter))
                .with_css(format!(
                    "text-align: center; cursor: pointer; {}",
                    if has { "font-weight: 700;" } else { "opacity: 0.35;" }
                ))
                .with_child(text(letter.to_string()))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    RefAny::new(LetterRef { letter }),
                    on_jump,
                ),
        );
    }
    bar
}

fn list_pane(s: &ContactsApp, app: &RefAny) -> Dom {
    let indices = s.view();
    let sections = book::sections(&s.book, &indices, s.sort);
    let present: Vec<char> = sections.iter().map(|(l, _)| *l).collect();
    let search = TextInput::create_search()
        .with_text(s.query.as_str())
        .with_placeholder(label("azcontacts-search"))
        .with_accessibility_name(label("azcontacts-search"))
        .with_on_text_input(app.clone(), on_search as TextInputOnTextInputCallbackType)
        .dom()
        .with_id(ids::CONTACTS_SEARCH);
    let sort = Segmented::create(strs(&["azcontacts-first-name", "azcontacts-last-name"]))
        .with_selected_index(usize::from(s.sort == SortBy::Last))
        .with_on_change(app.clone(), on_sort as SegmentedOnChangeCallbackType)
        .dom()
        .with_id(ids::CONTACTS_SORT);
    let heading = block(
        "padding: 6px 8px 2px 8px; font-size: 12px; font-weight: 600;",
        text(format!("{} \u{b7} {}", filter_title(s), indices.len())),
    )
    .with_id(ids::CONTACTS_HEADING);
    let body = if !s.loaded {
        block("padding: 16px; opacity: 0.7;", text(label("azcontacts-reading-contacts")))
    } else if s.book.is_empty() {
        ShellEmptyState::create(label("azcontacts-no-contacts"))
            .with_icon("contacts")
            .with_detail(label("azcontacts-no-contacts-detail"))
            .with_action_label(label("azcontacts-new-contact"))
            .with_on_action(app.clone(), on_new as ButtonOnClickCallbackType)
            .dom()
    } else if indices.is_empty() {
        ShellEmptyState::create(label("azcontacts-nobody-here"))
            .with_icon("search")
            .with_detail(if s.query.trim().is_empty() {
                t("azcontacts-group-empty")
            } else {
                t_args("azcontacts-no-match", &[("query", Arg::from(s.query.trim()))])
            })
            .dom()
    } else {
        let mut list = Dom::create_div()
            .with_id(ids::CONTACTS_LIST)
            .with_css("display: flex; flex-direction: column; flex-grow: 1; overflow-y: auto; min-height: 0px;");
        for (letter, members) in &sections {
            list.add_child(
                block(
                    "padding: 4px 8px; font-size: 11px; font-weight: 700; opacity: 0.8;",
                    text(letter.to_string()),
                )
                .with_id(ids::section(*letter)),
            );
            for &i in members {
                list.add_child(contact_row(s, app, &s.book[i]));
            }
        }
        flex_row("flex-grow: 1; min-height: 0px; align-items: stretch;", vec![list, jump_bar(&present)])
    };
    column(
        "flex-grow: 1; min-height: 0px;",
        vec![
            flex_row("padding: 6px 8px;", vec![block("flex-grow: 1; margin-right: 6px;", search), sort]),
            heading,
            body,
        ],
    )
}

// ==== The card ====

fn field_row(label: &str, value: Dom) -> Dom {
    flex_row(
        "align-items: flex-start; padding: 3px 0px;",
        vec![
            block("width: 96px; flex-shrink: 0; font-size: 12px; opacity: 0.7;", text(label_word(label))),
            block("flex-grow: 1; font-size: 13px;", value),
        ],
    )
}

fn lines(items: Vec<String>) -> Dom {
    column("", items.into_iter().map(|l| Dom::create_div().with_child(text(l))).collect())
}

fn card_view(s: &ContactsApp, app: &RefAny, c: &Contact) -> Dom {
    let mut actions = vec![
        primary("azcontacts-edit", ids::CARD_EDIT, app, on_edit),
        Button::create(format!("{} {}", if c.favorite { "\u{2605}" } else { "\u{2606}" }, t("azcontacts-favourite")))
            .with_on_click(app.clone(), on_toggle_favorite as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::CARD_FAVORITE),
        button("azcontacts-copy-vcard", ids::CARD_COPY, app, on_copy_vcard),
        button("azcontacts-export", ids::CARD_EXPORT, app, on_export_selected),
    ];
    if !c.emails.is_empty() {
        // Until AzMail takes a hand-off: the address to the clipboard.
        actions.push(
            Button::create(label("azcontacts-mail"))
                .with_icon("mail")
                .with_on_click(app.clone(), on_copy_email as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::CARD_MAIL),
        );
    }
    actions.push(
        Button::create(label("azcontacts-delete"))
            .with_button_type(ButtonType::Danger)
            .with_on_click(app.clone(), on_delete as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::CARD_DELETE),
    );
    let mut children = vec![
        flex_row(
            "padding: 12px 0px;",
            vec![
                avatar(s, c, AvatarSize::Large),
                column(
                    "padding-left: 12px;",
                    vec![
                        block("font-size: 22px; font-weight: 600;", text(shown_name(c))).with_id(ids::CARD_NAME),
                        block("font-size: 13px; opacity: 0.75;", text(c.subtitle())),
                    ],
                ),
            ],
        ),
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; flex-wrap: wrap; gap: 6px; padding-bottom: 8px;")
            .with_children(DomVec::from_vec(actions)),
    ];
    if s.confirm_delete {
        children.push(flex_row(
            "padding: 6px 0px;",
            vec![
                block(
                    "padding-right: 8px;",
                    text(t_args("azcontacts-delete-ask", &[("name", Arg::from(shown_name(c)))])),
                ),
                Button::create(label("azcontacts-delete"))
                    .with_button_type(ButtonType::Danger)
                    .with_on_click(app.clone(), on_delete_confirmed as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::CARD_DELETE_CONFIRM),
                button("azcontacts-keep", ids::CARD_DELETE_CANCEL, app, on_delete_cancelled),
            ],
        ));
    }
    let mut fields = Vec::new();
    for p in &c.phones {
        fields.push(field_row(&p.label, text(p.value.as_str())));
    }
    for e in &c.emails {
        fields.push(field_row(&e.label, text(e.value.as_str())));
    }
    for a in &c.addresses {
        fields.push(field_row(&a.label, lines(a.lines())));
    }
    for u in &c.urls {
        fields.push(field_row(&u.label, text(u.value.as_str())));
    }
    if let Some(b) = &c.birthday {
        fields.push(field_row("birthday", text(birthday_text(b))));
    }
    if !c.nickname.trim().is_empty() {
        fields.push(field_row("nickname", text(c.nickname.as_str())));
    }
    if !c.groups.is_empty() {
        let chips: Vec<Dom> = c.groups.iter().map(|g| Chip::create(g.as_str()).dom()).collect();
        fields.push(field_row(
            "groups",
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; flex-wrap: wrap; gap: 4px;")
                .with_children(DomVec::from_vec(chips)),
        ));
    }
    for f in &c.custom {
        fields.push(field_row(&f.label, text(f.value.as_str())));
    }
    if !c.photo.trim().is_empty() {
        let photo = if c.photo.starts_with("data:") { t("azcontacts-picture-in-card") } else { c.photo.clone() };
        fields.push(field_row("photo", text(photo)));
    }
    if !c.notes.trim().is_empty() {
        fields.push(field_row("notes", lines(c.notes.lines().map(str::to_string).collect())));
    }
    children.push(column("", fields).with_id(ids::CARD_FIELDS));
    children.push(block(
        "padding-top: 12px; font-size: 11px; opacity: 0.6;",
        text(t_args("azcontacts-file", &[("file", Arg::from(store::contact_key(&c.uid)))])),
    ));
    column("padding: 0px 16px; overflow-y: auto; flex-grow: 1; min-height: 0px;", children).with_id(ids::CONTACT_CARD)
}

// ==== The edit form ====

/// A text field of the form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormField {
    Given,
    Family,
    Org,
    Department,
    Title,
    Nickname,
    Birthday,
    NewGroup,
    Phone(usize),
    Email(usize),
    Street(usize),
    Postcode(usize),
    City(usize),
    Region(usize),
    Country(usize),
    CustomLabel(usize),
    CustomValue(usize),
    ImportPath,
    ImportGroup,
}

/// A repeated row of the form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    Phone,
    Email,
    Address,
    Custom,
    Group,
}

struct FieldRef {
    app: RefAny,
    field: FormField,
}

struct LabelRef {
    app: RefAny,
    kind: RowKind,
    index: usize,
}

struct RowKindRef {
    app: RefAny,
    kind: RowKind,
    index: usize,
}

fn input(app: &RefAny, field: FormField, value: &str, placeholder: &str, id: AzString) -> Dom {
    TextInput::create()
        .with_text(value)
        .with_placeholder(label(placeholder))
        .with_accessibility_name(label(placeholder))
        .with_on_text_input(
            RefAny::new(FieldRef { app: app.clone(), field }),
            on_form_text as TextInputOnTextInputCallbackType,
        )
        .dom()
        .with_id(id)
}

fn label_drop(app: &RefAny, kind: RowKind, index: usize, labels: &[&str], current: &str, id: AzString) -> Dom {
    let mut choices: Vec<&str> = labels.to_vec();
    if !choices.contains(&current) && !current.is_empty() {
        choices.push(current);
    }
    let selected = choices.iter().position(|l| *l == current).unwrap_or(0);
    let said: Vec<String> = choices.iter().map(|l| label_word(l)).collect();
    let said: Vec<&str> = said.iter().map(String::as_str).collect();
    DropDown::create(strs(&said))
        .with_selected(selected)
        .with_accessibility_name(label("azcontacts-label"))
        .with_on_choice_change(
            RefAny::new(LabelRef { app: app.clone(), kind, index }),
            on_label_change as DropDownOnChoiceChangeCallbackType,
        )
        .dom()
        .with_id(id)
}

fn remove_button(app: &RefAny, kind: RowKind, index: usize, id: AzString) -> Dom {
    Button::create("")
        .with_icon("remove_circle_outline")
        .with_on_click(RefAny::new(RowKindRef { app: app.clone(), kind, index }), on_remove_row as ButtonOnClickCallbackType)
        .dom()
        .with_id(id)
}

fn add_button(app: &RefAny, kind: RowKind, text: &str, id: AzString) -> Dom {
    Button::create(label(text))
        .with_icon("add")
        .with_on_click(RefAny::new(RowKindRef { app: app.clone(), kind, index: 0 }), on_add_row as ButtonOnClickCallbackType)
        .dom()
        .with_id(id)
}

fn form_section(title: &str, children: Vec<Dom>) -> Dom {
    let title = t(title).to_uppercase();
    let mut all = vec![block("padding: 10px 0px 4px 0px; font-size: 11px; font-weight: 700; opacity: 0.7;", text(title))];
    all.extend(children);
    column("", all)
}

/// The labels a picked label list offers (the label itself if it is the user's own).
fn labels_for(kind: RowKind) -> &'static [&'static str] {
    match kind {
        RowKind::Phone => &PHONE_LABELS,
        RowKind::Email => &EMAIL_LABELS,
        _ => &ADDRESS_LABELS,
    }
}

/// The birthday on a calendar: azul's DatePicker on the birthday's month (a birthday
/// without a year in a leap year), its year as a number, "Year unknown"; without a birthday,
/// a button that starts one. Each sets the form's text (`DD.MM.YYYY` / `DD.MM.`).
fn birthday_picker(app: &RefAny, form: &Form) -> Dom {
    let Some(b) = Birthday::parse(&form.birthday_text) else {
        return flex_row(
            "padding-top: 6px;",
            vec![button("azcontacts-add-birthday", ids::EDIT_BIRTHDAY_ADD, app, on_birthday_add)],
        );
    };
    let year = u32::try_from(b.picker_year()).unwrap_or(2000);
    let mut controls = vec![
        DatePicker::create(year, b.month, b.day)
            .with_accessibility_name(label("azcontacts-birthday"))
            .with_on_change(app.clone(), on_birthday_picked as DatePickerOnChangeCallbackType)
            .dom()
            .with_id(ids::EDIT_BIRTHDAY_PICKER),
    ];
    let mut side = vec![flex_row(
        "gap: 6px; align-items: center;",
        vec![
            CheckBox::create(b.year.is_none())
                .with_accessibility_name(label("azcontacts-year-unknown"))
                .with_on_toggle(app.clone(), on_birthday_no_year as CheckBoxOnToggleCallbackType)
                .dom()
                .with_id(ids::EDIT_BIRTHDAY_NO_YEAR),
            text(label("azcontacts-year-unknown")),
        ],
    )];
    if let Some(y) = b.year {
        // The calendar's arrows step months: a year decades back is typed.
        side.push(flex_row(
            "gap: 6px; align-items: center;",
            vec![
                text(label("azcontacts-year")),
                NumberInput::create(y as f32)
                    .with_accessibility_name(label("azcontacts-birth-year"))
                    .with_on_value_change(app.clone(), on_birthday_year as NumberInputOnValueChangeCallbackType)
                    .dom()
                    .with_id(ids::EDIT_BIRTHDAY_YEAR),
            ],
        ));
    }
    controls.push(column("gap: 8px; padding-left: 12px;", side));
    flex_row("padding-top: 6px; align-items: flex-start;", controls)
}

fn edit_view(s: &ContactsApp, app: &RefAny, form: &Form) -> Dom {
    let d = &form.draft;
    let mut children = Vec::new();
    children.push(flex_row(
        "padding: 10px 0px;",
        vec![
            block(
                "font-size: 18px; font-weight: 600; flex-grow: 1;",
                text(label(if form.original.is_some() { "azcontacts-edit-contact" } else { "azcontacts-new-contact" })),
            ),
            button("kit-button-cancel", ids::EDIT_CANCEL, app, on_edit_cancel),
            primary("azcontacts-save", ids::EDIT_SAVE, app, on_edit_save),
        ],
    ));
    let mut photo_row = vec![
        avatar(s, d, AvatarSize::Medium),
        button("azcontacts-change-photo", ids::EDIT_PHOTO, app, on_photo_choose),
    ];
    if !d.photo.trim().is_empty() {
        photo_row.push(button("azcontacts-remove-photo", ids::EDIT_PHOTO_REMOVE, app, on_photo_remove));
        photo_row.push(block("font-size: 12px; opacity: 0.75;", text(label("azcontacts-photo-set"))));
    }
    children.push(flex_row("gap: 8px; padding-bottom: 6px;", photo_row));
    if form.confirm_discard {
        children.push(flex_row(
            "padding: 6px 0px;",
            vec![
                block("padding-right: 8px;", text(label("azcontacts-discard-ask"))),
                Button::create(label("azcontacts-discard"))
                    .with_button_type(ButtonType::Danger)
                    .with_on_click(app.clone(), on_edit_discard as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::EDIT_DISCARD),
                button("azcontacts-keep-editing", ids::EDIT_KEEP, app, on_edit_keep),
            ],
        ));
    }
    if !form.problems.is_empty() {
        children.push(
            // Under flora a problem is marked in its clay stone.
            column(
                "padding: 6px 8px; border-left: 3px solid #c0392b; @theme(flora) { border-left: \
                 3px solid #7E4A42; @media (prefers-color-scheme: dark) { border-left: 3px solid \
                 #B3837A; } }",
                form.problems.iter().map(|p| Dom::create_div().with_child(text(t_text(p)))).collect(),
            )
                .with_id(ids::EDIT_PROBLEMS),
        );
    }
    let pair = |a: Dom, b: Dom| flex_row("gap: 6px; padding: 2px 0px;", vec![block("flex-grow: 1;", a), block("flex-grow: 1;", b)]);
    children.push(form_section(
        "azcontacts-name",
        vec![
            pair(
                input(app, FormField::Given, &d.given, "azcontacts-first-name", ids::EDIT_GIVEN),
                input(app, FormField::Family, &d.family, "azcontacts-last-name", ids::EDIT_FAMILY),
            ),
            pair(
                input(app, FormField::Org, &d.org, "azcontacts-company", ids::EDIT_ORG),
                input(app, FormField::Department, &d.department, "azcontacts-department", ids::EDIT_DEPARTMENT),
            ),
            pair(
                input(app, FormField::Title, &d.title, "azcontacts-job-title", ids::EDIT_TITLE),
                input(app, FormField::Nickname, &d.nickname, "azcontacts-nickname", ids::EDIT_NICKNAME),
            ),
        ],
    ));
    let mut phones: Vec<Dom> = d
        .phones
        .iter()
        .enumerate()
        .map(|(i, p)| {
            flex_row(
                "gap: 6px; padding: 2px 0px;",
                vec![
                    label_drop(app, RowKind::Phone, i, &PHONE_LABELS, &p.label, ids::edit_phone_label(i)),
                    block("flex-grow: 1;", input(app, FormField::Phone(i), &p.value, "azcontacts-phone", ids::edit_phone(i))),
                    remove_button(app, RowKind::Phone, i, ids::edit_phone_remove(i)),
                ],
            )
        })
        .collect();
    phones.push(add_button(app, RowKind::Phone, "azcontacts-add-phone", ids::EDIT_ADD_PHONE));
    children.push(form_section("azcontacts-phone", phones));
    let mut emails: Vec<Dom> = d
        .emails
        .iter()
        .enumerate()
        .map(|(i, e)| {
            flex_row(
                "gap: 6px; padding: 2px 0px;",
                vec![
                    label_drop(app, RowKind::Email, i, &EMAIL_LABELS, &e.label, ids::edit_email_label(i)),
                    block("flex-grow: 1;", input(app, FormField::Email(i), &e.value, "azcontacts-email", ids::edit_email(i))),
                    remove_button(app, RowKind::Email, i, ids::edit_email_remove(i)),
                ],
            )
        })
        .collect();
    emails.push(add_button(app, RowKind::Email, "azcontacts-add-email", ids::EDIT_ADD_EMAIL));
    children.push(form_section("azcontacts-email", emails));
    let mut addresses: Vec<Dom> = Vec::new();
    for (i, a) in d.addresses.iter().enumerate() {
        addresses.push(flex_row(
            "gap: 6px; padding: 2px 0px;",
            vec![
                label_drop(app, RowKind::Address, i, &ADDRESS_LABELS, &a.label, ids::edit_address_label(i)),
                block("flex-grow: 1;", input(app, FormField::Street(i), &a.street, "azcontacts-street", ids::edit_street(i))),
                remove_button(app, RowKind::Address, i, ids::edit_address_remove(i)),
            ],
        ));
        addresses.push(flex_row(
            "gap: 6px; padding: 2px 0px 6px 0px;",
            vec![
                block("width: 90px;", input(app, FormField::Postcode(i), &a.postcode, "azcontacts-postcode", ids::edit_postcode(i))),
                block("flex-grow: 1;", input(app, FormField::City(i), &a.locality, "azcontacts-city", ids::edit_city(i))),
                block("flex-grow: 1;", input(app, FormField::Region(i), &a.region, "azcontacts-region", ids::edit_region(i))),
                block("flex-grow: 1;", input(app, FormField::Country(i), &a.country, "azcontacts-country", ids::edit_country(i))),
            ],
        ));
    }
    addresses.push(add_button(app, RowKind::Address, "azcontacts-add-address", ids::EDIT_ADD_ADDRESS));
    children.push(form_section("azcontacts-address", addresses));
    children.push(form_section(
        "azcontacts-birthday",
        vec![
            input(app, FormField::Birthday, &form.birthday_text, "azcontacts-birthday-format", ids::EDIT_BIRTHDAY),
            birthday_picker(app, form),
        ],
    ));
    let chips: Vec<Dom> = d
        .groups
        .iter()
        .enumerate()
        .map(|(i, g)| {
            Chip::create(g.as_str())
                .with_removable(true)
                .with_on_remove(RefAny::new(RowKindRef { app: app.clone(), kind: RowKind::Group, index: i }), on_remove_group as ChipOnRemoveCallbackType)
                .dom()
        })
        .collect();
    children.push(form_section(
        "azcontacts-groups",
        vec![
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; flex-wrap: wrap; gap: 4px; padding-bottom: 4px;")
                .with_children(DomVec::from_vec(chips)),
            flex_row(
                "gap: 6px;",
                vec![
                    block("flex-grow: 1;", input(app, FormField::NewGroup, &form.new_group, "azcontacts-add-to-a-group", ids::EDIT_NEW_GROUP)),
                    add_button(app, RowKind::Group, "azcontacts-add", ids::EDIT_ADD_GROUP),
                ],
            ),
        ],
    ));
    let mut custom: Vec<Dom> = d
        .custom
        .iter()
        .enumerate()
        .map(|(i, f)| {
            flex_row(
                "gap: 6px; padding: 2px 0px;",
                vec![
                    block("width: 140px;", input(app, FormField::CustomLabel(i), &f.label, "azcontacts-field-name", ids::edit_field_label(i))),
                    block("flex-grow: 1;", input(app, FormField::CustomValue(i), &f.value, "azcontacts-value", ids::edit_field(i))),
                    remove_button(app, RowKind::Custom, i, ids::edit_field_remove(i)),
                ],
            )
        })
        .collect();
    custom.push(add_button(app, RowKind::Custom, "azcontacts-add-field", ids::EDIT_ADD_FIELD));
    children.push(form_section("azcontacts-more-fields", custom));
    children.push(form_section(
        "azcontacts-notes",
        vec![TextArea::create()
            .with_text(d.notes.as_str())
            .with_placeholder(label("azcontacts-notes"))
            .with_accessibility_name(label("azcontacts-notes"))
            .with_on_text_input(app.clone(), on_notes as TextAreaOnTextInputCallbackType)
            .dom()
            .with_id(ids::EDIT_NOTES)],
    ));
    children.push(flex_row(
        "padding: 8px 0px;",
        vec![
            Switch::create(d.favorite)
                .with_accessibility_name(label("azcontacts-favourite"))
                .with_on_toggle(app.clone(), on_form_favorite as SwitchOnToggleCallbackType)
                .dom()
                .with_id(ids::EDIT_FAVORITE),
            block("padding-left: 8px;", text(label("azcontacts-favourite"))),
        ],
    ));
    column("padding: 0px 16px 16px 16px; overflow-y: auto; flex-grow: 1; min-height: 0px;", children).with_id(ids::CONTACT_EDIT)
}

// ==== Import ====

struct ImportRowRef {
    app: RefAny,
    index: usize,
}

fn status_text(status: &ImportStatus, book: &[Contact]) -> String {
    match status {
        ImportStatus::New => t("azcontacts-status-new"),
        ImportStatus::Update(i) => t_args(
            "azcontacts-status-updates",
            &[("name", Arg::from(book.get(*i).map(shown_name).unwrap_or_default()))],
        ),
        ImportStatus::Duplicate(i, score) => t_args(
            "azcontacts-status-duplicate",
            &[
                ("name", Arg::from(book.get(*i).map(shown_name).unwrap_or_default())),
                ("score", Arg::from(format!("{score:.2}"))),
            ],
        ),
    }
}

/// A CSV column of the import preview.
struct ImportColumnRef {
    app: RefAny,
    index: usize,
}

/// The CSV columns and what each becomes: the header, an example value, a drop-down of the
/// contact's fields (`#import-column-<n>`).
fn csv_mapping(app: &RefAny, csv: &CsvImport) -> Dom {
    let labels: Vec<&str> = crate::csv::Field::ALL.iter().map(|f| f.label()).collect();
    let mut rows = vec![block("padding: 8px 0px 4px 0px; font-weight: 600;", text(label("azcontacts-columns")))];
    for (i, header) in csv.table.headers.iter().enumerate() {
        let field = csv.mapping.get(i).copied().unwrap_or(crate::csv::Field::Skip);
        let example = csv
            .table
            .rows
            .iter()
            .map(|r| r.get(i).map(String::as_str).unwrap_or_default().trim())
            .find(|v| !v.is_empty())
            .unwrap_or("\u{2014}")
            .to_string();
        rows.push(flex_row(
            "gap: 8px; padding: 2px 0px; font-size: 13px;",
            vec![
                block("width: 140px; flex-shrink: 0;", text(header.as_str())),
                // The first value of the column, cut short where the row has no room for it.
                block(
                    "flex-grow: 1; min-width: 0px; opacity: 0.7; white-space: nowrap; overflow: hidden; \
                     text-overflow: ellipsis;",
                    text(example),
                ),
                DropDown::create(strs(&labels))
                    .with_selected(field.index())
                    .with_accessibility_name(t_args("azcontacts-column", &[("header", Arg::from(header.as_str()))]))
                    .with_on_choice_change(
                        RefAny::new(ImportColumnRef { app: app.clone(), index: i }),
                        on_import_column as DropDownOnChoiceChangeCallbackType,
                    )
                    .dom()
                    .with_id(ids::import_column(i))
                    .with_css("flex-shrink: 0;"),
            ],
        ));
    }
    column("", rows).with_id(ids::IMPORT_COLUMNS)
}

/// A column of the import preview: `grow` shares of the row (a zero basis, so the shares hold
/// whatever the values say), a value too long for its share cut with an ellipsis.
fn preview_cell(grow: u32, css: &str) -> String {
    format!(
        "flex-grow: {grow}; flex-basis: 0px; min-width: 0px; white-space: nowrap; overflow: hidden; \
         text-overflow: ellipsis; {css}"
    )
}

fn import_view(s: &ContactsApp, app: &RefAny, st: &ImportState) -> Dom {
    let mut children = vec![
        block("font-size: 18px; font-weight: 600; padding: 10px 0px;", text(label("azcontacts-import-contacts"))),
        flex_row(
            "gap: 6px;",
            vec![
                block("flex-grow: 1;", input(app, FormField::ImportPath, &st.path, "azcontacts-import-path", ids::IMPORT_PATH)),
                button("azcontacts-read", ids::IMPORT_READ, app, on_import_read),
                button("azcontacts-choose-file", ids::IMPORT_CHOOSE, app, on_import_choose),
            ],
        ),
    ];
    if st.reading {
        children.push(block("padding: 8px 0px;", text(label("azcontacts-reading"))));
    }
    for p in &st.problems {
        children.push(block("font-size: 12px; opacity: 0.8;", text(t_text(p))));
    }
    if let Some(csv) = &st.csv {
        children.push(csv_mapping(app, csv));
    }
    if !st.rows.is_empty() {
        children.push(
            block("padding: 8px 0px; font-weight: 600;", text(t_text(&store::import_summary(&st.rows))))
                .with_id(ids::IMPORT_SUMMARY),
        );
        let mut table = Vec::new();
        for (i, r) in st.rows.iter().enumerate() {
            let c = &r.contact;
            table.push(flex_row(
                "gap: 8px; padding: 2px 0px; font-size: 13px;",
                vec![
                    CheckBox::create(r.selected)
                        .with_accessibility_name(t_args("azcontacts-import-one", &[("name", Arg::from(shown_name(c)))]))
                        .with_on_toggle(RefAny::new(ImportRowRef { app: app.clone(), index: i }), on_import_toggle as CheckBoxOnToggleCallbackType)
                        .dom()
                        .with_id(ids::import_row(i)),
                    block(&preview_cell(3, ""), text(shown_name(c))),
                    block(&preview_cell(3, "opacity: 0.8;"), text(c.emails.first().map(|e| e.value.clone()).unwrap_or_else(|| "\u{2014}".into()))),
                    block(&preview_cell(2, "opacity: 0.8;"), text(c.phones.first().map(|p| p.value.clone()).unwrap_or_else(|| "\u{2014}".into()))),
                    block(&preview_cell(3, "opacity: 0.8;"), text(status_text(&r.status, &s.book))),
                ],
            ));
        }
        children.push(column("", table).with_id(ids::IMPORT_ROWS));
        children.push(flex_row(
            "gap: 6px; padding-top: 10px;",
            vec![
                block("", text(label("azcontacts-add-to-group"))),
                block("flex-grow: 1;", input(app, FormField::ImportGroup, &st.group, "azcontacts-group-optional", ids::IMPORT_GROUP)),
                button("kit-button-cancel", ids::IMPORT_CANCEL, app, on_import_cancel),
                primary("azcontacts-import", ids::IMPORT_RUN, app, on_import_run),
            ],
        ));
    } else if !st.reading {
        children.push(block(
            "padding-top: 12px; opacity: 0.75; font-size: 13px;",
            text(label("azcontacts-import-what")),
        ));
        children.push(flex_row("padding-top: 8px;", vec![button("kit-button-cancel", ids::IMPORT_CANCEL, app, on_import_cancel)]));
    }
    column("padding: 0px 16px 16px 16px; overflow-y: auto; flex-grow: 1; min-height: 0px;", children).with_id(ids::CONTACT_IMPORT)
}

// ==== Merge ====

/// A single field of the merge screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeField {
    Name,
    Company,
    Birthday,
    Photo,
    Notes,
}

struct MergeRef {
    app: RefAny,
    field: MergeField,
}

/// A merge field's left or right value: the two share the row's room evenly whatever they say
/// (a zero basis), so every field's Left / Right picker stands in one column.
const MERGE_VALUE: &str = "flex-grow: 1; flex-basis: 0px; min-width: 0px; font-size: 13px;";

fn merge_view(s: &ContactsApp, app: &RefAny, st: &MergeState) -> Dom {
    let Some(pair) = st.pairs.get(st.index) else {
        return ShellEmptyState::create(label("azcontacts-no-duplicates"))
            .with_icon("merge")
            .with_detail(label("azcontacts-no-duplicates-detail"))
            .dom()
            .with_id(ids::MERGE_EMPTY);
    };
    let (a, b) = (&s.book[pair.a], &s.book[pair.b]);
    let pick_row = |label: &str, field: MergeField, pick: Pick, left: String, right: String, id: AzString| {
        flex_row(
            "gap: 8px; padding: 4px 0px;",
            vec![
                block("width: 90px; font-size: 12px; opacity: 0.7;", text(azul_appkit::l10n::label(label))),
                block(MERGE_VALUE, text(if left.is_empty() { "\u{2014}".to_string() } else { left })),
                Segmented::create(strs(&["azcontacts-left", "azcontacts-right"]))
                    .with_selected_index(usize::from(pick == Pick::B))
                    .with_on_change(RefAny::new(MergeRef { app: app.clone(), field }), on_merge_pick as SegmentedOnChangeCallbackType)
                    .dom()
                    .with_id(id),
                block(MERGE_VALUE, text(if right.is_empty() { "\u{2014}".to_string() } else { right })),
            ],
        )
    };
    let company = |c: &Contact| c.subtitle();
    let birthday = |c: &Contact| c.birthday.map(|b| birthday_text(&b)).unwrap_or_default();
    let photo = |c: &Contact| if c.photo.is_empty() { String::new() } else { label_word("photo") };
    let list = |c: &Contact| -> Vec<String> {
        c.phones
            .iter()
            .map(|p| format!("{}: {}", label_word(&p.label), p.value))
            .chain(c.emails.iter().map(|e| format!("{}: {}", label_word(&e.label), e.value)))
            .chain(c.addresses.iter().map(|x| format!("{}: {}", label_word(&x.label), x.lines().join(", "))))
            .collect()
    };
    let children = vec![
        flex_row(
            "padding: 10px 0px; gap: 8px;",
            vec![
                block(
                    "font-size: 18px; font-weight: 600; flex-grow: 1;",
                    text(t_args("azcontacts-duplicates-count", &[("count", Arg::from(st.pairs.len()))])),
                ),
                button("\u{2039}", ids::MERGE_PREV, app, on_merge_prev),
                block(
                    "font-size: 13px;",
                    text(t_args(
                        "azcontacts-pair-of",
                        &[("pair", Arg::from(st.index + 1)), ("pairs", Arg::from(st.pairs.len()))],
                    )),
                )
                .with_id(ids::MERGE_POSITION),
                button("\u{203a}", ids::MERGE_NEXT, app, on_merge_next),
            ],
        ),
        block(
            "font-size: 13px; padding-bottom: 8px;",
            text(t_args(
                "azcontacts-pair",
                &[
                    ("a", Arg::from(shown_name(a))),
                    ("b", Arg::from(shown_name(b))),
                    ("score", Arg::from(format!("{:.2}", pair.score))),
                    ("why", Arg::from(pair.reasons.iter().map(t_text).collect::<Vec<_>>().join(", "))),
                ],
            )),
        )
        .with_id(ids::MERGE_PAIR),
        pick_row("azcontacts-name", MergeField::Name, st.plan.name, shown_name(a), shown_name(b), ids::MERGE_NAME),
        pick_row("azcontacts-company", MergeField::Company, st.plan.company, company(a), company(b), ids::MERGE_COMPANY),
        pick_row("azcontacts-birthday", MergeField::Birthday, st.plan.birthday, birthday(a), birthday(b), ids::MERGE_BIRTHDAY),
        pick_row("azcontacts-photo", MergeField::Photo, st.plan.photo, photo(a), photo(b), ids::MERGE_PHOTO),
        pick_row("azcontacts-notes", MergeField::Notes, st.plan.notes, a.notes.clone(), b.notes.clone(), ids::MERGE_NOTES),
        flex_row(
            "padding: 4px 0px 4px 98px;",
            vec![
                Switch::create(st.plan.notes_both)
                    .with_accessibility_name(label("azcontacts-keep-both-notes"))
                    .with_on_toggle(app.clone(), on_merge_notes_both as SwitchOnToggleCallbackType)
                    .dom()
                    .with_id(ids::MERGE_NOTES_BOTH),
                block("padding-left: 8px; font-size: 13px;", text(label("azcontacts-keep-both-notes"))),
            ],
        ),
        block(
            "padding: 8px 0px 2px 0px; font-size: 11px; font-weight: 700; opacity: 0.7;",
            text(t("azcontacts-kept-from-both").to_uppercase()),
        ),
        flex_row(
            "align-items: flex-start; gap: 16px;",
            vec![
                block("flex-grow: 1; font-size: 12px;", lines(list(a))),
                block("flex-grow: 1; font-size: 12px;", lines(list(b))),
            ],
        ),
        flex_row(
            "gap: 6px; padding-top: 12px;",
            vec![
                block("flex-grow: 1;", Dom::create_div()),
                button("azcontacts-not-a-duplicate", ids::MERGE_IGNORE, app, on_merge_ignore),
                primary("azcontacts-merge-contacts", ids::MERGE_RUN, app, on_merge_run),
            ],
        ),
    ];
    column("padding: 0px 16px 16px 16px; overflow-y: auto; flex-grow: 1; min-height: 0px;", children).with_id(ids::CONTACT_MERGE)
}

// ==== The panes and the window ====

fn reading_pane(s: &ContactsApp, app: &RefAny) -> Dom {
    match &s.reading {
        Reading::Edit(form) => edit_view(s, app, form),
        Reading::Import(st) => import_view(s, app, st),
        Reading::Merge(st) => merge_view(s, app, st),
        Reading::Card => match s.selected_index() {
            Some(i) => card_view(s, app, &s.book[i]),
            None => ShellEmptyState::create(label("azcontacts-none-selected"))
                .with_icon("person")
                .with_detail(label("azcontacts-none-selected-detail"))
                .with_action_label(label("azcontacts-new-contact"))
                .with_on_action(app.clone(), on_new as ButtonOnClickCallbackType)
                .dom(),
        },
    }
}

/// The toolbar in the ribbon row: azul's `Toolbar` (roving focus, the "more" menu). Each tool's
/// `id` is its DOM-id name from [`ids`] (`__azcontacts_toolbar-*`, what the E2E clicks): what
/// [`on_toolbar`] matches.
fn toolbar(app: &RefAny) -> Dom {
    let tool = |id: AzString, text: &str, icon: &str| {
        ToolbarItem::create_button(id, label(text), icon).with_show_label(true)
    };
    let items = vec![
        tool(ids::TOOLBAR_NEW, "azcontacts-new", "person_add"),
        tool(ids::TOOLBAR_IMPORT, "azcontacts-import", "file_upload"),
        tool(ids::TOOLBAR_EXPORT, "azcontacts-export", "file_download"),
        tool(ids::TOOLBAR_DUPLICATES, "azcontacts-duplicates", "merge"),
        ToolbarItem::create_spacer(),
        tool(ids::TOOLBAR_SETTINGS, "azcontacts-settings", "settings"),
    ];
    block(
        "padding: 4px 8px;",
        Toolbar::create(label("azcontacts-contacts"))
            .with_items(items)
            .with_on_event(app.clone(), on_toolbar as ToolbarOnEventCallbackType)
            .dom(),
    )
}

/// A tool was pressed: the tool's `id` names the command.
extern "C" fn on_toolbar(data: RefAny, info: CallbackInfo, event: ToolbarEvent) -> Update {
    if event.kind != ToolbarEventKind::Activate {
        return Update::DoNothing;
    }
    let id = event.id.as_str();
    let command: ButtonOnClickCallbackType = if id == ids::TOOLBAR_NEW.as_str() {
        on_new
    } else if id == ids::TOOLBAR_IMPORT.as_str() {
        on_import_open
    } else if id == ids::TOOLBAR_EXPORT.as_str() {
        on_export_view
    } else if id == ids::TOOLBAR_DUPLICATES.as_str() {
        on_open_duplicates
    } else if id == ids::TOOLBAR_SETTINGS.as_str() {
        on_open_settings
    } else {
        return Update::DoNothing;
    };
    command(data, info)
}

fn status_bar(s: &ContactsApp, app: &RefAny) -> Dom {
    let dupes = s.duplicates().len();
    let mut segments = vec![StatusBarSegment::create(t_args(
        "azcontacts-status-contacts",
        &[("count", Arg::from(s.book.len()))],
    ))];
    if dupes > 0 {
        segments.push(
            StatusBarSegment::create(t_args("azcontacts-status-duplicates", &[("count", Arg::from(dupes))]))
                .with_on_click(app.clone(), on_open_duplicates as ButtonOnClickCallbackType),
        );
    }
    if !s.notice.is_empty() {
        segments.push(StatusBarSegment::create(s.notice.as_str()));
    }
    StatusBar::create(segments).dom().with_id(ids::CONTACTS_STATUS)
}

fn settings_sections(s: &ContactsApp, app: &RefAny) -> Vec<AppSection> {
    vec![AppSection {
        category: 0,
        title: "azcontacts-list-and-files".to_string(),
        content: column(
            "",
            vec![
                kit::row(
                    "azcontacts-sort-by",
                    Segmented::create(strs(&["azcontacts-first-name", "azcontacts-last-name"]))
                        .with_selected_index(usize::from(s.sort == SortBy::Last))
                        .with_on_change(app.clone(), on_sort as SegmentedOnChangeCallbackType)
                        .dom()
                        .with_id(ids::SET_SORT),
                ),
                kit::row(
                    "azcontacts-export-as",
                    Segmented::create(strs(&["vCard 4.0", "vCard 3.0"]))
                        .with_selected_index(usize::from(s.export_version == Version::V3))
                        .with_on_change(app.clone(), on_export_version as SegmentedOnChangeCallbackType)
                        .dom()
                        .with_id(ids::SET_EXPORT_VERSION),
                ),
                kit::note(&t_args(
                    "azcontacts-files-note",
                    &[
                        (
                            "folder",
                            Arg::from(azul_appkit::data::local_path(&s.data_root, store::APP_FOLDER).display().to_string()),
                        ),
                        ("count", Arg::from(s.ignored.len())),
                    ],
                )),
            ],
        ),
    }]
}

/// The window: the shell (or the settings page), the theme scope, the window keys.
extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window; the layout's language
    // says the words (a switch of it too).
    let _mode = info.get_mode();
    azul_appkit::l10n::begin_layout(&info);
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<ContactsApp>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    let content = if kit::settings_open(&s.kit) {
        column(
            "flex-grow: 1; min-height: 0px;",
            vec![
                kit::title_row(SPEC.name),
                kit::settings_page_with_reload(
                    &s.kit,
                    settings_sections(s, &app),
                    &app,
                    reload_settings,
                ),
            ],
        )
    } else {
        PimShell::create(navigation(s, &app), list_pane(s, &app), reading_pane(s, &app))
            .with_list_label(label("azcontacts-contacts"))
            .office_shell()
            .with_title_row(kit::title_row(SPEC.name))
            .with_ribbon(toolbar(&app))
            .with_status_bar(status_bar(s, &app))
            .dom()
    };
    let root = column("flex-grow: 1; min-height: 0px;", vec![content]);
    // The theme scope's own body (SMALL6): no UA margin, the window's full height.
    ShellThemeScope::create(root)
        .with_accent(ShellThemeAccent::Blue)
        .body()
        .with_callback(EventFilter::Window(WindowEventFilter::VirtualKeyDown), app, on_key)
}

// ==== Callbacks: files ====

/// Runs `f` on the app's state; the window is rebuilt afterwards.
fn with_app(
    app: &mut RefAny,
    info: &mut CallbackInfo,
    f: impl FnOnce(&mut ContactsApp, &mut CallbackInfo, &RefAny),
) -> Update {
    let handle = app.clone();
    let Some(mut guard) = app.downcast_mut::<ContactsApp>() else {
        return Update::DoNothing;
    };
    f(&mut guard, info, &handle);
    refresh_photos(&mut guard);
    Update::RefreshDom
}

/// The photo as a picture azul shows: decoded and scaled to the largest avatar.
fn decode_photo(photo: &str) -> Option<ImageRef> {
    let bytes = crate::photo::image_bytes(photo)?;
    let image = match RawImage::decode_image_bytes_any(U8VecRef::from(bytes.as_slice())) {
        ResultRawImageDecodeImageError::Ok(image) => image,
        ResultRawImageDecodeImageError::Err(_) => return None,
    };
    let image = image.thumbnail(PHOTO_PX, PHOTO_PX).into_option()?;
    ImageRef::create_rawimage(image).into_option()
}

/// The largest a decoded photo is kept, in pixels (the large avatar, on a 2x screen).
const PHOTO_PX: u32 = 160;

/// Decodes the photos the window shows next - the list's visible contacts are many, so only
/// the selected contact's and the form's - once each (`photo::KEPT` at most).
fn refresh_photos(s: &mut ContactsApp) {
    let mut wanted: Vec<String> = Vec::new();
    if let Some(c) = s.selected.as_ref().and_then(|uid| s.book.iter().find(|c| &c.uid == uid)) {
        wanted.push(c.photo.clone());
    }
    if let Reading::Edit(form) = &s.reading {
        wanted.push(form.draft.photo.clone());
    }
    for photo in wanted.into_iter().filter(|p| !p.trim().is_empty()) {
        let key = crate::photo::key(&photo);
        if s.photos.contains_key(&key) {
            continue;
        }
        if s.photos.len() >= crate::photo::KEPT {
            s.photos.clear();
        }
        s.photos.insert(key, decode_photo(&photo));
    }
}

/// The contact's avatar: its photo when one is decoded, else its initials.
fn avatar(s: &ContactsApp, c: &Contact, size: AvatarSize) -> Dom {
    let mut a = Avatar::create(book::initials(c)).with_size(size);
    if let Some(Some(image)) = s.photos.get(&crate::photo::key(&c.photo)) {
        a = a.with_image(image.clone());
    }
    a.dom()
}

fn write_files(s: &ContactsApp, info: &mut CallbackInfo, app: &RefAny, jobs: Vec<FileJob>, tag: u64) {
    kit::spawn_file_jobs(info, &s.data_root, jobs, app.clone(), tag, on_files_done);
}

/// Puts a contact into the book (replacing the one with its UID) and writes its file.
fn save_contact(s: &mut ContactsApp, info: &mut CallbackInfo, app: &RefAny, mut c: Contact) {
    store::ensure_uid(&mut c);
    let (key, bytes) = store::file_of(&c);
    match s.book.iter().position(|x| x.uid == c.uid) {
        Some(i) => s.book[i] = c.clone(),
        None => s.book.push(c.clone()),
    }
    write_files(s, info, app, vec![FileJob::Put { key, bytes }], TAG_WRITE);
    s.select(Some(c.uid));
}

fn copy_to_clipboard(info: &mut CallbackInfo, text: &str) {
    info.set_clipboard_content(ClipboardContent {
        plain_text: AzString::from(text),
        styled_runs: StyledTextRunVec::create(),
        html: OptionString::None,
    });
}

/// Writes the contacts at `indices` as one `.vcf` under `exports/` in the data root.
fn export_contacts(s: &mut ContactsApp, info: &mut CallbackInfo, app: &RefAny, indices: &[usize]) {
    if indices.is_empty() {
        s.notice = t("azcontacts-nothing-to-export");
        return;
    }
    let text = store::export(&s.book, indices, s.export_version);
    let key = format!("exports/contacts-{}.vcf", now_secs());
    s.notice = t_args(
        "azcontacts-exporting",
        &[
            ("count", Arg::from(indices.len())),
            ("path", Arg::from(azul_appkit::data::local_path(&s.data_root, &key).display().to_string())),
        ],
    );
    write_files(s, info, app, vec![FileJob::Put { key, bytes: text.into_bytes() }], TAG_WRITE);
}

/// Reads a `.vcf` file for the import preview (on a Thread; the drive is its folder).
fn read_import_file(s: &mut ContactsApp, info: &mut CallbackInfo, app: &RefAny, path: &Path) {
    let (Some(folder), Some(name)) = (path.parent(), path.file_name()) else {
        if let Reading::Import(st) = &mut s.reading {
            st.problems = vec![Phrase::new("azcontacts-import-not-a-file").arg("path", path.display().to_string()).into()];
        }
        return;
    };
    let folder = if folder.as_os_str().is_empty() { Path::new(".") } else { folder };
    let mut state = match std::mem::replace(&mut s.reading, Reading::Card) {
        Reading::Import(st) => st,
        _ => empty_import(),
    };
    state.path = path.display().to_string();
    state.rows.clear();
    state.problems.clear();
    state.csv = None;
    state.reading = true;
    s.reading = Reading::Import(state);
    kit::spawn_file_jobs(
        info,
        folder,
        vec![FileJob::Get { key: name.to_string_lossy().into_owned() }],
        app.clone(),
        TAG_IMPORT_FILE,
        on_files_done,
    );
}

/// After loading: the screen `--screen` asked for, the files given to import.
fn after_load(s: &mut ContactsApp, info: &mut CallbackInfo, app: &RefAny) {
    if s.selected.is_none() {
        let first = s.view().first().map(|&i| s.book[i].uid.clone());
        s.select(first);
    }
    match std::mem::take(&mut s.start_screen).as_str() {
        "new" => s.reading = Reading::Edit(Form::new(None)),
        "duplicates" => open_duplicates(s),
        "import" => s.reading = Reading::Import(empty_import()),
        _ => {}
    }
    if let Some(path) = s.import_files.first().cloned() {
        s.import_files.clear();
        read_import_file(s, info, app, &path);
    }
}

fn empty_import() -> ImportState {
    ImportState {
        path: String::new(),
        rows: Vec::new(),
        problems: Vec::new(),
        // A group's name in the files: the default in the window's language.
        group: t("azcontacts-imported-group"),
        reading: false,
        csv: None,
    }
}

fn open_duplicates(s: &mut ContactsApp) {
    let pairs = s.duplicates();
    println!("AZCONTACTS_DUPLICATES {}", pairs.len());
    s.reading = Reading::Merge(MergeState {
        pairs,
        index: 0,
        plan: MergePlan::default(),
    });
}

extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(s) = data.downcast_ref::<ContactsApp>() else {
        return Update::DoNothing;
    };
    kit::on_window_created(&s.kit, &mut info);
    kit::spawn_file_jobs(
        &mut info,
        &s.data_root,
        vec![FileJob::GetAll {
            prefix: format!("{}/", store::APP_FOLDER),
            suffix: store::SUFFIX.to_string(),
        }],
        app.clone(),
        TAG_LOAD,
        on_files_done,
    );
    Update::DoNothing
}

extern "C" fn on_files_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, info, handle| match reply.tag {
        TAG_LOAD => {
            let mut files = Vec::new();
            for outcome in reply.outcomes {
                if let FileOutcome::GotAll { files: f, errors, .. } = outcome {
                    files = f;
                    for e in errors {
                        eprintln!("[azcontacts] {e}");
                    }
                }
            }
            let (book, problems) = store::load(&files);
            for p in &problems {
                eprintln!("[azcontacts] {p}");
            }
            if !problems.is_empty() {
                s.notice = t_args("azcontacts-files-not-read", &[("count", Arg::from(problems.len()))]);
            }
            s.book = book;
            if s.book.is_empty() && s.sample {
                s.book = sample::sample_book();
                let jobs: Vec<FileJob> = s
                    .book
                    .iter()
                    .map(|c| {
                        let (key, bytes) = store::file_of(c);
                        FileJob::Put { key, bytes }
                    })
                    .collect();
                write_files(s, info, handle, jobs, TAG_SAMPLE);
            }
            s.loaded = true;
            println!("AZCONTACTS_LOADED {}", s.book.len());
            after_load(s, info, handle);
        }
        TAG_IMPORT_FILE => {
            let mut text = None;
            let mut problem = None;
            for outcome in reply.outcomes {
                match outcome {
                    FileOutcome::Got { result: Ok(Some(bytes)), .. } => text = Some(String::from_utf8_lossy(&bytes).into_owned()),
                    FileOutcome::Got { result: Ok(None), key } => {
                        problem = Some(Text::from(Phrase::new("azcontacts-file-missing").arg("file", key.as_str())));
                    }
                    // The file thread's own words.
                    FileOutcome::Got { result: Err(e), .. } => problem = Some(Text::plain(e)),
                    _ => {}
                }
            }
            let book = s.book.clone();
            if let Reading::Import(st) = &mut s.reading {
                st.reading = false;
                if let Some(p) = problem {
                    st.problems = vec![p];
                }
                if let Some(text) = text {
                    // A CSV file: its columns mapped by their headers, the user changes the
                    // mapping in the preview.
                    let (rows, problems) = if is_csv(&st.path) {
                        match crate::csv::parse(&text) {
                            Ok(table) => {
                                let mapping: Vec<crate::csv::Field> =
                                    table.headers.iter().map(|h| crate::csv::guess(h)).collect();
                                let preview = store::csv_preview(&table, &mapping, &book);
                                st.csv = Some(CsvImport { table, mapping });
                                preview
                            }
                            // The CSV reader's own words (appkit's csv).
                            Err(e) => (Vec::new(), vec![Text::plain(e)]),
                        }
                    } else {
                        store::import_preview(&text, &book)
                    };
                    if rows.is_empty() && problems.is_empty() {
                        st.problems.push(Text::key(if st.csv.is_some() {
                            "azcontacts-import-no-person"
                        } else {
                            "azcontacts-import-no-vcard"
                        }));
                    }
                    st.problems.extend(problems);
                    st.rows = rows;
                    println!("AZCONTACTS_IMPORT_PREVIEW {} {}", st.rows.len(), t_text(&store::import_summary(&st.rows)));
                }
            }
        }
        TAG_SAMPLE | TAG_WRITE => {
            let mut failed = 0;
            for outcome in &reply.outcomes {
                if let Some(e) = outcome.error() {
                    failed += 1;
                    eprintln!("[azcontacts] {e}");
                    continue;
                }
                match outcome {
                    FileOutcome::Put { key, .. } if reply.tag == TAG_WRITE => {
                        if let Some(uid) = key.strip_prefix("contacts/").and_then(|k| k.strip_suffix(".vcf")) {
                            println!("AZCONTACTS_SAVED {uid}");
                        } else {
                            println!("AZCONTACTS_EXPORTED {key}");
                            s.notice = t_args(
                                "azcontacts-exported",
                                &[("path", Arg::from(azul_appkit::data::local_path(&s.data_root, key).display().to_string()))],
                            );
                        }
                    }
                    FileOutcome::Deleted { key, .. } => {
                        if let Some(uid) = store::uid_of_key(key) {
                            println!("AZCONTACTS_DELETED {uid}");
                        }
                    }
                    _ => {}
                }
            }
            if reply.tag == TAG_SAMPLE {
                println!("AZCONTACTS_SAMPLE_WRITTEN {}", reply.outcomes.len() - failed);
            }
            if failed > 0 {
                s.notice = t_args("azcontacts-not-written", &[("count", Arg::from(failed))]);
            }
        }
        _ => {}
    })
}

// ==== Callbacks: navigation and list ====

/// Leaves the reading pane's form or screen for the card; `false` when the
/// edit form has changes (it asks first).
fn leave_reading(s: &mut ContactsApp) -> bool {
    if let Reading::Edit(form) = &mut s.reading {
        if form.changed() {
            form.confirm_discard = true;
            return false;
        }
    }
    s.reading = Reading::Card;
    true
}

fn set_filter(s: &mut ContactsApp, filter: Filter) {
    if !leave_reading(s) {
        return;
    }
    s.filter = filter;
    let view = s.view();
    println!("AZCONTACTS_VIEW {}", view.len());
    if !s.selected_index().is_some_and(|i| view.contains(&i)) {
        let first = view.first().map(|&i| s.book[i].uid.clone());
        s.select(first);
    }
}

extern "C" fn on_nav(mut data: RefAny, mut info: CallbackInfo, event: ShellNavigationPaneEvent) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| match event.kind {
        ShellNavigationPaneEventKind::GroupToggled => {
            if event.group < s.nav_open.len() {
                s.nav_open[event.group] = event.expand;
            }
        }
        ShellNavigationPaneEventKind::NodeClicked => match (event.group, event.index) {
            (0, 1) => set_filter(s, Filter::Favorites),
            (0, 2) => {
                if leave_reading(s) {
                    open_duplicates(s);
                }
            }
            (1, k) if k >= 1 => {
                let counts = book::group_counts(&s.book);
                if let Some((name, _)) = counts.get(k - 1) {
                    set_filter(s, Filter::Group(name.clone()));
                }
            }
            _ => set_filter(s, Filter::All),
        },
        _ => {}
    })
}

extern "C" fn on_row(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, uid)) = data.downcast_ref::<RowRef>().map(|r| (r.app.clone(), r.uid.clone())) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, _info, _| {
        if leave_reading(s) {
            s.select(Some(uid));
        }
    })
}

/// The A-Z bar: the letter's section (or the next one with contacts) scrolls to the top.
extern "C" fn on_jump(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(letter) = data.downcast_ref::<LetterRef>().map(|l| l.letter) else {
        return Update::DoNothing;
    };
    let dom = info.get_hit_node().dom;
    let start = ALPHABET.iter().position(|l| *l == letter).unwrap_or(0);
    for candidate in ALPHABET[start..].iter().chain(ALPHABET[..start].iter().rev()) {
        let node = info.get_node_id_by_id_attribute(dom, ids::section(*candidate));
        if node.into_raw() != 0 {
            info.scroll_node_into_view(DomNodeId { dom, node }, ScrollIntoViewOptions::start());
            println!("AZCONTACTS_JUMP {candidate}");
            break;
        }
    }
    Update::DoNothing
}

extern "C" fn on_search(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let query = state.get_text().as_str().to_string();
    let update = with_app(&mut data, &mut info, |s, _info, _| {
        s.query = query;
        let view = s.view();
        println!("AZCONTACTS_VIEW {}", view.len());
        if !s.selected_index().is_some_and(|i| view.contains(&i)) && matches!(s.reading, Reading::Card) {
            let first = view.first().map(|&i| s.book[i].uid.clone());
            s.select(first);
        }
    });
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// The list's order the settings name (first names unless `sort` is `last`).
fn sort_of(settings: &AppSettings) -> SortBy {
    if settings.get("sort") == Some("last") {
        SortBy::Last
    } else {
        SortBy::First
    }
}

/// The vCard version an export writes (4.0 unless `export` is `3.0`).
fn export_version_of(settings: &AppSettings) -> Version {
    if settings.get("export") == Some("3.0") {
        Version::V3
    } else {
        Version::V4
    }
}

/// Cancel on the settings page put the settings back: the order and the export version follow.
fn reload_settings(app: &mut RefAny, _info: &mut CallbackInfo, settings: &AppSettings) {
    if let Some(mut s) = app.downcast_mut::<ContactsApp>() {
        s.sort = sort_of(settings);
        s.export_version = export_version_of(settings);
    };
}

extern "C" fn on_sort(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        s.sort = if state.selected_index == 1 { SortBy::Last } else { SortBy::First };
        kit::set_value(&s.kit, info, "sort", s.sort.key());
    })
}

extern "C" fn on_open_settings(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| kit::open_settings(&s.kit, None))
}

extern "C" fn on_export_version(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        s.export_version = if state.selected_index == 1 { Version::V3 } else { Version::V4 };
        kit::set_value(&s.kit, info, "export", s.export_version.label());
    })
}

// ==== Callbacks: the card ====

extern "C" fn on_new(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        if leave_reading(s) {
            s.reading = Reading::Edit(Form::new(None));
        }
    })
}

extern "C" fn on_edit(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        if let Some(i) = s.selected_index() {
            s.reading = Reading::Edit(Form::new(Some(&s.book[i])));
        }
    })
}

extern "C" fn on_toggle_favorite(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        if let Some(i) = s.selected_index() {
            let mut c = s.book[i].clone();
            c.favorite = !c.favorite;
            save_contact(s, info, handle, c);
        }
    })
}

extern "C" fn on_copy_vcard(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        if let Some(i) = s.selected_index() {
            copy_to_clipboard(info, &s.book[i].to_vcf(s.export_version));
            s.notice = t_args(
                "azcontacts-copied-vcard",
                &[("name", Arg::from(shown_name(&s.book[i]))), ("version", Arg::from(s.export_version.label()))],
            );
        }
    })
}

extern "C" fn on_copy_email(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        if let Some(e) = s.selected_index().and_then(|i| s.book[i].emails.first().cloned()) {
            copy_to_clipboard(info, &e.value);
            s.notice = t_args("azcontacts-copied", &[("what", Arg::from(e.value.as_str()))]);
        }
    })
}

extern "C" fn on_export_selected(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        let indices: Vec<usize> = s.selected_index().into_iter().collect();
        export_contacts(s, info, handle, &indices);
    })
}

extern "C" fn on_export_view(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        let indices = s.view();
        export_contacts(s, info, handle, &indices);
    })
}

extern "C" fn on_delete(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| s.confirm_delete = true)
}

extern "C" fn on_delete_cancelled(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| s.confirm_delete = false)
}

extern "C" fn on_delete_confirmed(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        let Some(i) = s.selected_index() else {
            return;
        };
        let view = s.view();
        let pos = view.iter().position(|&x| x == i).unwrap_or(0);
        let removed = s.book.remove(i);
        write_files(s, info, handle, vec![FileJob::Delete { key: store::contact_key(&removed.uid) }], TAG_WRITE);
        let view = s.view();
        let next = view.get(pos.min(view.len().saturating_sub(1))).map(|&j| s.book[j].uid.clone());
        s.select(next);
        s.notice = t_args("azcontacts-deleted", &[("name", Arg::from(shown_name(&removed)))]);
    })
}

// ==== Callbacks: the edit form ====

/// Runs `f` on the edit form, if the reading pane shows one.
fn with_form(app: &mut RefAny, info: &mut CallbackInfo, f: impl FnOnce(&mut Form)) -> Update {
    with_app(app, info, |s, _info, _| {
        if let Reading::Edit(form) = &mut s.reading {
            f(form);
        }
    })
}

/// A day picked on the birthday's calendar (or its month turned): the birthday is that day,
/// its year kept unknown when it was.
extern "C" fn on_birthday_picked(mut data: RefAny, mut info: CallbackInfo, state: DatePickerState) -> Update {
    with_form(&mut data, &mut info, |form| {
        let year_known = Birthday::parse(&form.birthday_text).map_or(true, |b| b.year.is_some());
        let year = i32::try_from(state.year).unwrap_or(2000);
        if let Some(b) = Birthday::picked(year, state.month, state.day, year_known) {
            form.birthday_text = b.to_form();
        }
    })
}

/// "Year unknown": the birthday loses its year, or gets the one the calendar shows.
extern "C" fn on_birthday_no_year(mut data: RefAny, mut info: CallbackInfo, state: CheckBoxState) -> Update {
    with_form(&mut data, &mut info, |form| {
        if let Some(b) = Birthday::parse(&form.birthday_text) {
            if let Some(next) = Birthday::picked(b.picker_year(), b.month, b.day, !state.checked) {
                form.birthday_text = next.to_form();
            }
        }
    })
}

/// The birth year typed.
extern "C" fn on_birthday_year(mut data: RefAny, mut info: CallbackInfo, state: NumberInputState) -> Update {
    with_form(&mut data, &mut info, |form| {
        let year = state.number.round();
        if !(1.0..=9999.0).contains(&year) {
            return;
        }
        if let Some(b) = Birthday::parse(&form.birthday_text) {
            // 29 February in a year without one: the 28th.
            let picked = Birthday::picked(year as i32, b.month, b.day, true)
                .or_else(|| Birthday::picked(year as i32, b.month, b.day.saturating_sub(1), true));
            if let Some(next) = picked {
                form.birthday_text = next.to_form();
            }
        }
    })
}

/// "Add a birthday": 1 January, year unknown, to change on the calendar.
extern "C" fn on_birthday_add(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_form(&mut data, &mut info, |form| {
        form.birthday_text = Birthday { year: None, month: 1, day: 1 }.to_form();
    })
}

fn set_field(form: &mut Form, field: FormField, value: String) {
    let d = &mut form.draft;
    match field {
        FormField::Given => d.given = value,
        FormField::Family => d.family = value,
        FormField::Org => d.org = value,
        FormField::Department => d.department = value,
        FormField::Title => d.title = value,
        FormField::Nickname => d.nickname = value,
        FormField::Birthday => form.birthday_text = value,
        FormField::NewGroup => form.new_group = value,
        FormField::Phone(i) => {
            if let Some(p) = d.phones.get_mut(i) {
                p.value = value;
            }
        }
        FormField::Email(i) => {
            if let Some(e) = d.emails.get_mut(i) {
                e.value = value;
            }
        }
        FormField::Street(i) => {
            if let Some(a) = d.addresses.get_mut(i) {
                a.street = value;
            }
        }
        FormField::Postcode(i) => {
            if let Some(a) = d.addresses.get_mut(i) {
                a.postcode = value;
            }
        }
        FormField::City(i) => {
            if let Some(a) = d.addresses.get_mut(i) {
                a.locality = value;
            }
        }
        FormField::Region(i) => {
            if let Some(a) = d.addresses.get_mut(i) {
                a.region = value;
            }
        }
        FormField::Country(i) => {
            if let Some(a) = d.addresses.get_mut(i) {
                a.country = value;
            }
        }
        FormField::CustomLabel(i) => {
            if let Some(f) = d.custom.get_mut(i) {
                f.label = value;
            }
        }
        FormField::CustomValue(i) => {
            if let Some(f) = d.custom.get_mut(i) {
                f.value = value;
            }
        }
        FormField::ImportPath | FormField::ImportGroup => {}
    }
}

/// A text field of the form or the import screen. The draft takes the text; the
/// window is not rebuilt (the field shows what was typed).
extern "C" fn on_form_text(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let keep = OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    };
    let Some((mut app, field)) = data.downcast_ref::<FieldRef>().map(|f| (f.app.clone(), f.field)) else {
        return keep;
    };
    let value = state.get_text().as_str().to_string();
    let _ = with_app(&mut app, &mut info, |s, _info, _| match (&mut s.reading, field) {
        (Reading::Import(st), FormField::ImportPath) => st.path = value,
        (Reading::Import(st), FormField::ImportGroup) => st.group = value,
        (Reading::Edit(form), field) => set_field(form, field, value),
        _ => {}
    });
    keep
}

extern "C" fn on_notes(mut data: RefAny, mut info: CallbackInfo, state: TextAreaState) -> OnTextInputReturn {
    let notes: String = state.get_text().to_string();
    let _ = with_form(&mut data, &mut info, |form| form.draft.notes = notes);
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_form_favorite(mut data: RefAny, mut info: CallbackInfo, state: SwitchState) -> Update {
    with_form(&mut data, &mut info, |form| form.draft.favorite = state.checked)
}

extern "C" fn on_label_change(mut data: RefAny, mut info: CallbackInfo, choice: usize) -> Update {
    let Some((mut app, kind, index)) = data.downcast_ref::<LabelRef>().map(|l| (l.app.clone(), l.kind, l.index)) else {
        return Update::DoNothing;
    };
    with_form(&mut app, &mut info, |form| {
        let d = &mut form.draft;
        let current = match kind {
            RowKind::Phone => d.phones.get(index).map(|p| p.label.clone()),
            RowKind::Email => d.emails.get(index).map(|e| e.label.clone()),
            RowKind::Address => d.addresses.get(index).map(|a| a.label.clone()),
            _ => None,
        };
        let Some(current) = current else {
            return;
        };
        // The same choices label_drop offered: the standard labels, then the row's own.
        let mut choices: Vec<String> = labels_for(kind).iter().map(|l| (*l).to_string()).collect();
        if !choices.contains(&current) && !current.is_empty() {
            choices.push(current);
        }
        let Some(label) = choices.get(choice).cloned() else {
            return;
        };
        match kind {
            RowKind::Phone => d.phones[index].label = label,
            RowKind::Email => d.emails[index].label = label,
            RowKind::Address => d.addresses[index].label = label,
            _ => {}
        }
    })
}

extern "C" fn on_add_row(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, kind)) = data.downcast_ref::<RowKindRef>().map(|r| (r.app.clone(), r.kind)) else {
        return Update::DoNothing;
    };
    with_form(&mut app, &mut info, |form| {
        let d = &mut form.draft;
        match kind {
            RowKind::Phone => {
                let label = if d.phones.is_empty() { "mobile" } else { "work" };
                d.phones.push(Labeled::new(label, ""));
            }
            RowKind::Email => {
                let label = if d.emails.is_empty() { "home" } else { "work" };
                d.emails.push(Labeled::new(label, ""));
            }
            RowKind::Address => d.addresses.push(Address {
                label: if d.addresses.is_empty() { "home".into() } else { "work".into() },
                ..Address::default()
            }),
            RowKind::Custom => d.custom.push(Labeled::new("", "")),
            RowKind::Group => {
                let group = form.new_group.trim().to_string();
                if !group.is_empty() && !form.draft.groups.contains(&group) {
                    form.draft.groups.push(group);
                }
                form.new_group.clear();
            }
        }
    })
}

extern "C" fn on_remove_row(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, kind, index)) = data.downcast_ref::<RowKindRef>().map(|r| (r.app.clone(), r.kind, r.index)) else {
        return Update::DoNothing;
    };
    with_form(&mut app, &mut info, |form| {
        let d = &mut form.draft;
        let len = match kind {
            RowKind::Phone => d.phones.len(),
            RowKind::Email => d.emails.len(),
            RowKind::Address => d.addresses.len(),
            RowKind::Custom => d.custom.len(),
            RowKind::Group => d.groups.len(),
        };
        if index >= len {
            return;
        }
        match kind {
            RowKind::Phone => {
                d.phones.remove(index);
            }
            RowKind::Email => {
                d.emails.remove(index);
            }
            RowKind::Address => {
                d.addresses.remove(index);
            }
            RowKind::Custom => {
                d.custom.remove(index);
            }
            RowKind::Group => {
                d.groups.remove(index);
            }
        }
    })
}

extern "C" fn on_remove_group(data: RefAny, info: CallbackInfo, _state: ChipState) -> Update {
    on_remove_row(data, info)
}

/// The draft as it is saved: empty rows dropped, the birthday from its text,
/// a group still in the "add" field added.
#[must_use]
pub fn finished_draft(form: &Form) -> (Contact, Vec<Text>) {
    let mut c = form.draft.clone();
    c.phones.retain(|p| !p.value.trim().is_empty());
    c.emails.retain(|e| !e.value.trim().is_empty());
    c.urls.retain(|u| !u.value.trim().is_empty());
    c.addresses.retain(|a| !a.is_empty());
    c.custom.retain(|f| !f.value.trim().is_empty());
    for p in c.phones.iter_mut().chain(c.emails.iter_mut()) {
        p.value = p.value.trim().to_string();
    }
    let group = form.new_group.trim();
    if !group.is_empty() && !c.groups.iter().any(|g| g == group) {
        c.groups.push(group.to_string());
    }
    c.birthday = Birthday::parse(&form.birthday_text);
    let problems = c.problems(Some(&form.birthday_text));
    (c, problems)
}

extern "C" fn on_edit_save(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        let Reading::Edit(form) = &mut s.reading else {
            return;
        };
        let (contact, problems) = finished_draft(form);
        if !problems.is_empty() {
            let said: Vec<String> = problems.iter().map(t_text).collect();
            println!("AZCONTACTS_PROBLEMS {}", said.join(" | "));
            form.problems = problems;
            return;
        }
        s.reading = Reading::Card;
        save_contact(s, info, handle, contact);
    })
}

extern "C" fn on_edit_cancel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        leave_reading(s);
    })
}

extern "C" fn on_edit_discard(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| s.reading = Reading::Card)
}

extern "C" fn on_edit_keep(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_form(&mut data, &mut info, |form| form.confirm_discard = false)
}

/// The image type of a picture file by its name.
#[must_use]
pub fn image_mime(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "image/jpeg"
    }
}

extern "C" fn on_photo_choose(mut data: RefAny, _info: CallbackInfo) -> Update {
    let app = data.clone();
    if data.downcast_ref::<ContactsApp>().is_none() {
        return Update::DoNothing;
    }
    let _request = FileDialog::open_file(
        label("azcontacts-choose-photo"),
        OptionString::None,
        OptionFileTypeList::None,
        app,
        on_photo_picked,
    );
    Update::DoNothing
}

/// The picture is read as it is picked (the dialog's answer arrives in a
/// callback, the file is small): the bytes become the card's data: URI.
extern "C" fn on_photo_picked(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing; // cancelled
    };
    let path = PathBuf::from(path.as_string().as_str());
    with_app(&mut data, &mut info, |s, info, handle| {
        let (Some(folder), Some(name)) = (path.parent(), path.file_name()) else {
            return;
        };
        kit::spawn_file_jobs(
            info,
            folder,
            vec![FileJob::Get { key: name.to_string_lossy().into_owned() }],
            handle.clone(),
            TAG_PHOTO,
            on_photo_read,
        );
        s.notice = t_args("azcontacts-reading-file", &[("path", Arg::from(path.display().to_string()))]);
    })
}

const TAG_PHOTO: u64 = 5;

extern "C" fn on_photo_read(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, _info, _| {
        for outcome in reply.outcomes {
            if let FileOutcome::Got { key, result } = outcome {
                match result {
                    Ok(Some(bytes)) if bytes.len() <= 2 * 1024 * 1024 => {
                        if let Reading::Edit(form) = &mut s.reading {
                            form.draft.photo = azul_pim::data_uri::data_uri(image_mime(&key), &bytes);
                            s.notice = t("azcontacts-photo-done");
                        }
                    }
                    Ok(Some(_)) => s.notice = t("azcontacts-photo-too-large"),
                    Ok(None) => s.notice = t_args("azcontacts-file-missing", &[("file", Arg::from(key.as_str()))]),
                    // The file thread's own words.
                    Err(e) => s.notice = e,
                }
            }
        }
    })
}

extern "C" fn on_photo_remove(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_form(&mut data, &mut info, |form| form.draft.photo.clear())
}

// ==== Callbacks: import ====

extern "C" fn on_import_open(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        if leave_reading(s) {
            s.reading = Reading::Import(empty_import());
        }
    })
}

extern "C" fn on_import_read(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        let path = match &s.reading {
            Reading::Import(st) => st.path.trim().to_string(),
            _ => return,
        };
        if path.is_empty() {
            if let Reading::Import(st) = &mut s.reading {
                st.problems = vec![Text::key("azcontacts-import-type-path")];
            }
            return;
        }
        read_import_file(s, info, handle, &PathBuf::from(path));
    })
}

extern "C" fn on_import_choose(mut data: RefAny, _info: CallbackInfo) -> Update {
    let app = data.clone();
    if data.downcast_ref::<ContactsApp>().is_none() {
        return Update::DoNothing;
    }
    let _request = FileDialog::open_file(
        label("azcontacts-import-contacts"),
        OptionString::None,
        OptionFileTypeList::None,
        app,
        on_import_file_picked,
    );
    Update::DoNothing
}

extern "C" fn on_import_file_picked(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing;
    };
    let path = PathBuf::from(path.as_string().as_str());
    with_app(&mut data, &mut info, |s, info, handle| read_import_file(s, info, handle, &path))
}

extern "C" fn on_import_toggle(mut data: RefAny, mut info: CallbackInfo, state: CheckBoxState) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<ImportRowRef>().map(|r| (r.app.clone(), r.index)) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, _info, _| {
        if let Reading::Import(st) = &mut s.reading {
            if let Some(r) = st.rows.get_mut(index) {
                r.selected = state.checked;
            }
        }
    })
}

/// A CSV column mapped to another field: the preview's rows are made again.
extern "C" fn on_import_column(mut data: RefAny, mut info: CallbackInfo, choice: usize) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<ImportColumnRef>().map(|r| (r.app.clone(), r.index)) else {
        return Update::DoNothing;
    };
    let Some(field) = crate::csv::Field::ALL.get(choice).copied() else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, _info, _| {
        let book = s.book.clone();
        if let Reading::Import(st) = &mut s.reading {
            if let Some(csv) = st.csv.as_mut() {
                if let Some(slot) = csv.mapping.get_mut(index) {
                    *slot = field;
                }
                let (rows, problems) = store::csv_preview(&csv.table, &csv.mapping, &book);
                st.rows = rows;
                st.problems = problems;
                println!("AZCONTACTS_IMPORT_PREVIEW {} {}", st.rows.len(), t_text(&store::import_summary(&st.rows)));
            }
        }
    })
}

extern "C" fn on_import_cancel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| s.reading = Reading::Card)
}

extern "C" fn on_import_run(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        let Reading::Import(st) = std::mem::replace(&mut s.reading, Reading::Card) else {
            return;
        };
        let group = st.group.trim().to_string();
        let mut jobs = Vec::new();
        let mut first = None;
        let mut count = 0;
        for row in st.rows.into_iter().filter(|r| r.selected) {
            let mut c = row.contact;
            store::ensure_uid(&mut c);
            if !group.is_empty() && !c.groups.contains(&group) {
                c.groups.push(group.clone());
            }
            match s.book.iter().position(|x| x.uid == c.uid) {
                Some(i) => s.book[i] = c.clone(),
                None => s.book.push(c.clone()),
            }
            let (key, bytes) = store::file_of(&c);
            jobs.push(FileJob::Put { key, bytes });
            first.get_or_insert(c.uid.clone());
            count += 1;
        }
        println!("AZCONTACTS_IMPORTED {count}");
        s.notice = t_args("azcontacts-imported", &[("count", Arg::from(count))]);
        write_files(s, info, handle, jobs, TAG_WRITE);
        if first.is_some() {
            s.select(first);
        }
    })
}

// ==== Callbacks: duplicates ====

extern "C" fn on_open_duplicates(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        if leave_reading(s) {
            open_duplicates(s);
        }
    })
}

fn with_merge(app: &mut RefAny, info: &mut CallbackInfo, f: impl FnOnce(&mut MergeState)) -> Update {
    with_app(app, info, |s, _info, _| {
        if let Reading::Merge(st) = &mut s.reading {
            f(st);
        }
    })
}

extern "C" fn on_merge_pick(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let Some((mut app, field)) = data.downcast_ref::<MergeRef>().map(|m| (m.app.clone(), m.field)) else {
        return Update::DoNothing;
    };
    let pick = if state.selected_index == 1 { Pick::B } else { Pick::A };
    with_merge(&mut app, &mut info, |st| match field {
        MergeField::Name => st.plan.name = pick,
        MergeField::Company => st.plan.company = pick,
        MergeField::Birthday => st.plan.birthday = pick,
        MergeField::Photo => st.plan.photo = pick,
        MergeField::Notes => st.plan.notes = pick,
    })
}

extern "C" fn on_merge_notes_both(mut data: RefAny, mut info: CallbackInfo, state: SwitchState) -> Update {
    with_merge(&mut data, &mut info, |st| st.plan.notes_both = state.checked)
}

extern "C" fn on_merge_prev(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_merge(&mut data, &mut info, |st| {
        if !st.pairs.is_empty() {
            st.index = (st.index + st.pairs.len() - 1) % st.pairs.len();
            st.plan = MergePlan::default();
        }
    })
}

extern "C" fn on_merge_next(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_merge(&mut data, &mut info, |st| {
        if !st.pairs.is_empty() {
            st.index = (st.index + 1) % st.pairs.len();
            st.plan = MergePlan::default();
        }
    })
}

/// The merge screen after the book changed: the pairs again, near the same place.
fn refresh_pairs(s: &mut ContactsApp, index: usize) {
    let pairs = s.duplicates();
    println!("AZCONTACTS_DUPLICATES {}", pairs.len());
    let index = index.min(pairs.len().saturating_sub(1));
    s.reading = Reading::Merge(MergeState {
        pairs,
        index,
        plan: MergePlan::default(),
    });
}

extern "C" fn on_merge_ignore(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        let Reading::Merge(st) = &s.reading else {
            return;
        };
        let Some(pair) = st.pairs.get(st.index).cloned() else {
            return;
        };
        let index = st.index;
        let (a, b) = (s.book[pair.a].uid.clone(), s.book[pair.b].uid.clone());
        s.ignored.push((a, b));
        let value = write_ignored(&s.ignored);
        kit::set_value(&s.kit, info, "ignored", &value);
        refresh_pairs(s, index);
    })
}

extern "C" fn on_merge_run(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        let Reading::Merge(st) = &s.reading else {
            return;
        };
        let Some(pair) = st.pairs.get(st.index).cloned() else {
            return;
        };
        let (index, plan) = (st.index, st.plan);
        let merged = dupes::merge(&s.book[pair.a], &s.book[pair.b], &plan);
        let gone = s.book[pair.b].uid.clone();
        s.book[pair.a] = merged.clone();
        s.book.remove(pair.b);
        let (key, bytes) = store::file_of(&merged);
        write_files(
            s,
            info,
            handle,
            vec![FileJob::Put { key, bytes }, FileJob::Delete { key: store::contact_key(&gone) }],
            TAG_WRITE,
        );
        println!("AZCONTACTS_MERGED {}", merged.uid);
        s.notice = t_args("azcontacts-merged", &[("name", Arg::from(shown_name(&merged)))]);
        s.selected = Some(merged.uid.clone());
        refresh_pairs(s, index);
    })
}

// ==== Keyboard ====

extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(kit_ref) = data.downcast_ref::<ContactsApp>().map(|s| s.kit.clone()) else {
        return Update::DoNothing;
    };
    if let Some(update) = kit::handle_key(&kit_ref, &mut info) {
        return update;
    }
    if kit::settings_open(&kit_ref) {
        return Update::DoNothing;
    }
    let Some(key) = info.get_current_keyboard_state().current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let m = info.get_key_modifiers();
    let command = m.primary_down();
    use azul::dom::VirtualKeyCode as K;
    match (key, command, m.shift) {
        (K::N, true, _) => {
            info.prevent_default();
            on_new(data, info)
        }
        (K::E, true, false) => {
            info.prevent_default();
            on_edit(data, info)
        }
        (K::E, true, true) => {
            info.prevent_default();
            on_export_view(data, info)
        }
        (K::S, true, _) => {
            info.prevent_default();
            on_edit_save(data, info)
        }
        (K::I, true, _) => {
            info.prevent_default();
            on_import_open(data, info)
        }
        (K::D, true, _) => {
            info.prevent_default();
            on_open_duplicates(data, info)
        }
        (K::Escape, false, _) => with_app(&mut data, &mut info, |s, _info, _| {
            leave_reading(s);
        }),
        (K::Up | K::Down, false, _) => with_app(&mut data, &mut info, |s, _info, _| {
            if !matches!(s.reading, Reading::Card) {
                return;
            }
            let view = s.view();
            if view.is_empty() {
                return;
            }
            let pos = s.selected_index().and_then(|i| view.iter().position(|&x| x == i));
            let next = match (pos, key == K::Down) {
                (None, _) => 0,
                (Some(p), true) => (p + 1).min(view.len() - 1),
                (Some(p), false) => p.saturating_sub(1),
            };
            let uid = s.book[view[next]].uid.clone();
            s.select(Some(uid));
        }),
        _ => Update::DoNothing,
    }
}

#[cfg(test)]
mod tests {
    //! The parts of the window that are plain data.

    use super::*;

    #[test]
    fn a_picture_file_names_its_image_type() {
        // The photo's base64 is azul_pim::data_uri's (tested there).
        assert_eq!(image_mime("Me.PNG"), "image/png");
        assert_eq!(image_mime("me.jpg"), "image/jpeg");
    }

    #[test]
    fn ignored_pairs_round_trip_through_the_settings_value() {
        let pairs = vec![("a".to_string(), "b".to_string()), ("c".to_string(), "d".to_string())];
        assert_eq!(write_ignored(&pairs), "a|b,c|d");
        assert_eq!(parse_ignored("a|b,c|d"), pairs);
        assert_eq!(parse_ignored(""), Vec::<(String, String)>::new());
        assert_eq!(parse_ignored("broken,|x,y|"), Vec::<(String, String)>::new());
    }

    #[test]
    fn a_saved_draft_drops_empty_rows_and_takes_the_birthday_text() {
        let mut form = Form::new(None);
        form.draft.given = "Robin".into();
        form.draft.phones = vec![Labeled::new("mobile", " +49 151 0000 0001 "), Labeled::new("work", "  ")];
        form.draft.emails = vec![Labeled::new("home", "")];
        form.birthday_text = "14.03.".into();
        form.new_group = "Book club".into();
        let (c, problems) = finished_draft(&form);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(c.phones, vec![Labeled::new("mobile", "+49 151 0000 0001")]);
        assert!(c.emails.is_empty());
        assert_eq!(c.birthday, Some(Birthday { year: None, month: 3, day: 14 }));
        assert_eq!(c.groups, vec!["Book club"]);
        assert!(form.changed());
        form.birthday_text = "32.13.".into();
        assert_eq!(finished_draft(&form).1.len(), 1);
        let empty = Form::new(None);
        assert_eq!(finished_draft(&empty).1, vec![Text::key("azcontacts-problem-no-name")]);
        assert!(!empty.changed());
    }

}
