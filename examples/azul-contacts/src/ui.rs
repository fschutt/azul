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

pub const SHORTCUTS: [Shortcut; 9] = [
    Shortcut::new("Contacts", "Mod+N", "New contact"),
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

// ==== Small pieces ====

fn strs(items: &[&str]) -> StringVec {
    StringVec::from_vec(items.iter().map(|s| AzString::from(*s)).collect())
}

fn text<S: Into<AzString>>(content: S) -> Dom {
    Dom::create_span_with_text(content)
}

fn block(css: &str, child: Dom) -> Dom {
    Dom::create_div().with_css(css).with_child(child)
}

fn column(css: &str, children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css(format!("display: flex; flex-direction: column; {css}"))
        .with_children(DomVec::from_vec(children))
}

fn row(css: &str, children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css(format!("display: flex; flex-direction: row; align-items: center; {css}"))
        .with_children(DomVec::from_vec(children))
}

fn button(label: &str, id: &str, app: &RefAny, cb: ButtonOnClickCallbackType) -> Dom {
    Button::create(label).with_on_click(app.clone(), cb).dom().with_id(id)
}

fn primary(label: &str, id: &str, app: &RefAny, cb: ButtonOnClickCallbackType) -> Dom {
    Button::create(label)
        .with_button_type(ButtonType::Primary)
        .with_on_click(app.clone(), cb)
        .dom()
        .with_id(id)
}

/// The id of a letter section (`#` is `hash`).
#[must_use]
pub fn section_id(letter: char) -> String {
    if letter == '#' {
        "section-hash".to_string()
    } else {
        format!("section-{letter}")
    }
}

// ==== Navigation ====

fn navigation(s: &ContactsApp, app: &RefAny) -> Dom {
    let favorites = s.book.iter().filter(|c| c.favorite).count();
    let dupes = s.duplicates().len();
    let duplicates_open = matches!(s.reading, Reading::Merge(_));
    let contacts = TreeViewNode::create(format!("All contacts ({})", s.book.len()))
        .with_icon("contacts")
        .with_expanded(true)
        .with_selected(s.filter == Filter::All && !duplicates_open)
        .with_child(
            TreeViewNode::create(format!("Favourites ({favorites})"))
                .with_icon("star")
                .with_selected(s.filter == Filter::Favorites && !duplicates_open),
        )
        .with_child(
            TreeViewNode::create(format!("Possible duplicates ({dupes})"))
                .with_icon("merge")
                .with_selected(duplicates_open),
        );
    let mut groups = TreeViewNode::create("Groups").with_icon("group").with_expanded(true);
    let counts = book::group_counts(&s.book);
    for (name, count) in &counts {
        groups = groups.with_child(
            TreeViewNode::create(format!("{name} ({count})"))
                .with_icon("label")
                .with_selected(s.filter == Filter::Group(name.clone()) && !duplicates_open),
        );
    }
    ShellNavigationPane::create()
        .with_header(primary("New contact", "contacts-new", app, on_new))
        .with_group(ShellNavigationGroup::create("Contacts", contacts).with_count(s.book.len()).with_open(s.nav_open[0]))
        .with_group(ShellNavigationGroup::create("Groups", groups).with_count(counts.len()).with_open(s.nav_open[1]))
        .with_label("Contacts and groups")
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
        Filter::All => "All contacts".to_string(),
        Filter::Favorites => "Favourites".to_string(),
        Filter::Group(g) => g.clone(),
    }
}

fn contact_row(s: &ContactsApp, app: &RefAny, c: &Contact) -> Dom {
    let selected = s.selected.as_deref() == Some(c.uid.as_str());
    let mut texts = vec![block("font-size: 13px;", text(c.display_name()))];
    let subtitle = c.subtitle();
    if !subtitle.is_empty() {
        texts.push(block("font-size: 11px; opacity: 0.7;", text(subtitle)));
    }
    let mut children = vec![
        Avatar::create(book::initials(c)).with_size(AvatarSize::Small).dom(),
        column("flex-grow: 1; padding-left: 8px; min-width: 0px;", texts),
    ];
    if c.favorite {
        children.push(block("padding: 0px 6px; font-size: 13px;", text("\u{2605}")));
    }
    row(
        &format!(
            "padding: 4px 8px; cursor: pointer; {}",
            if selected { "background-color: rgba(64, 128, 255, 0.18);" } else { "" }
        ),
        children,
    )
    .with_class("contact-row")
    .with_accessibility_name(c.display_name())
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
        .with_id("contacts-jump")
        .with_css("display: flex; flex-direction: column; width: 18px; flex-shrink: 0; font-size: 10px; padding-top: 4px;");
    for letter in ALPHABET {
        let has = present.contains(&letter);
        bar.add_child(
            Dom::create_div()
                .with_id(format!("jump-{}", if letter == '#' { "hash".to_string() } else { letter.to_string() }))
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
        .with_placeholder("Search contacts")
        .with_accessibility_name("Search contacts")
        .with_on_text_input(app.clone(), on_search as TextInputOnTextInputCallbackType)
        .dom()
        .with_id("contacts-search");
    let sort = Segmented::create(strs(&["First name", "Last name"]))
        .with_selected_index(usize::from(s.sort == SortBy::Last))
        .with_on_change(app.clone(), on_sort as SegmentedOnChangeCallbackType)
        .dom()
        .with_id("contacts-sort");
    let heading = block(
        "padding: 6px 8px 2px 8px; font-size: 12px; font-weight: 600;",
        text(format!("{} \u{b7} {}", filter_title(s), indices.len())),
    )
    .with_id("contacts-heading");
    let body = if !s.loaded {
        block("padding: 16px; opacity: 0.7;", text("Reading your contacts\u{2026}"))
    } else if s.book.is_empty() {
        ShellEmptyState::create("No contacts yet")
            .with_icon("contacts")
            .with_detail("Add one, or import a vCard file.")
            .with_action_label("New contact")
            .with_on_action(app.clone(), on_new as ButtonOnClickCallbackType)
            .dom()
    } else if indices.is_empty() {
        ShellEmptyState::create("Nobody here")
            .with_icon("search")
            .with_detail(if s.query.trim().is_empty() {
                "This group has no contacts.".to_string()
            } else {
                format!("No contact matches \u{201c}{}\u{201d}.", s.query.trim())
            })
            .dom()
    } else {
        let mut list = Dom::create_div()
            .with_id("contacts-list")
            .with_css("display: flex; flex-direction: column; flex-grow: 1; overflow-y: auto; min-height: 0px;");
        for (letter, members) in &sections {
            list.add_child(
                block(
                    "padding: 4px 8px; font-size: 11px; font-weight: 700; opacity: 0.8;",
                    text(letter.to_string()),
                )
                .with_id(section_id(*letter)),
            );
            for &i in members {
                list.add_child(contact_row(s, app, &s.book[i]));
            }
        }
        row("flex-grow: 1; min-height: 0px; align-items: stretch;", vec![list, jump_bar(&present)])
    };
    column(
        "flex-grow: 1; min-height: 0px;",
        vec![
            row("padding: 6px 8px;", vec![block("flex-grow: 1; margin-right: 6px;", search), sort]),
            heading,
            body,
        ],
    )
}

// ==== The card ====

fn field_row(label: &str, value: Dom) -> Dom {
    row(
        "align-items: flex-start; padding: 3px 0px;",
        vec![
            block("width: 96px; flex-shrink: 0; font-size: 12px; opacity: 0.7;", text(label)),
            block("flex-grow: 1; font-size: 13px;", value),
        ],
    )
}

fn lines(items: Vec<String>) -> Dom {
    column("", items.into_iter().map(|l| Dom::create_div().with_child(text(l))).collect())
}

fn card_view(s: &ContactsApp, app: &RefAny, c: &Contact) -> Dom {
    let mut actions = vec![
        primary("Edit", "card-edit", app, on_edit),
        Button::create(if c.favorite { "\u{2605} Favourite" } else { "\u{2606} Favourite" })
            .with_on_click(app.clone(), on_toggle_favorite as ButtonOnClickCallbackType)
            .dom()
            .with_id("card-favorite"),
        button("Copy vCard", "card-copy", app, on_copy_vcard),
        button("Export", "card-export", app, on_export_selected),
    ];
    if !c.emails.is_empty() {
        // Until AzMail takes a hand-off: the address to the clipboard.
        actions.push(
            Button::create("Mail")
                .with_icon("mail")
                .with_on_click(app.clone(), on_copy_email as ButtonOnClickCallbackType)
                .dom()
                .with_id("card-mail"),
        );
    }
    actions.push(
        Button::create("Delete")
            .with_button_type(ButtonType::Danger)
            .with_on_click(app.clone(), on_delete as ButtonOnClickCallbackType)
            .dom()
            .with_id("card-delete"),
    );
    let mut children = vec![
        row(
            "padding: 12px 0px;",
            vec![
                Avatar::create(book::initials(c)).with_size(AvatarSize::Large).dom(),
                column(
                    "padding-left: 12px;",
                    vec![
                        block("font-size: 22px; font-weight: 600;", text(c.display_name())).with_id("card-name"),
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
        children.push(row(
            "padding: 6px 0px;",
            vec![
                block("padding-right: 8px;", text(format!("Delete {}? Its file goes too.", c.display_name()))),
                Button::create("Delete")
                    .with_button_type(ButtonType::Danger)
                    .with_on_click(app.clone(), on_delete_confirmed as ButtonOnClickCallbackType)
                    .dom()
                    .with_id("card-delete-confirm"),
                button("Keep", "card-delete-cancel", app, on_delete_cancelled),
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
        fields.push(field_row("birthday", text(b.describe())));
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
        fields.push(field_row("photo", text(if c.photo.starts_with("data:") { "a picture in the card" } else { c.photo.as_str() })));
    }
    if !c.notes.trim().is_empty() {
        fields.push(field_row("notes", lines(c.notes.lines().map(str::to_string).collect())));
    }
    children.push(column("", fields).with_id("card-fields"));
    children.push(block(
        "padding-top: 12px; font-size: 11px; opacity: 0.6;",
        text(format!("File: {}", store::contact_key(&c.uid))),
    ));
    column("padding: 0px 16px; overflow-y: auto; flex-grow: 1; min-height: 0px;", children).with_id("contact-card")
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

fn input(app: &RefAny, field: FormField, value: &str, placeholder: &str, id: &str) -> Dom {
    TextInput::create()
        .with_text(value)
        .with_placeholder(placeholder)
        .with_accessibility_name(placeholder)
        .with_on_text_input(
            RefAny::new(FieldRef { app: app.clone(), field }),
            on_form_text as TextInputOnTextInputCallbackType,
        )
        .dom()
        .with_id(id)
}

fn label_drop(app: &RefAny, kind: RowKind, index: usize, labels: &[&str], current: &str, id: &str) -> Dom {
    let mut choices: Vec<&str> = labels.to_vec();
    if !choices.contains(&current) && !current.is_empty() {
        choices.push(current);
    }
    let selected = choices.iter().position(|l| *l == current).unwrap_or(0);
    DropDown::create(strs(&choices))
        .with_selected(selected)
        .with_accessibility_name("Label")
        .with_on_choice_change(
            RefAny::new(LabelRef { app: app.clone(), kind, index }),
            on_label_change as DropDownOnChoiceChangeCallbackType,
        )
        .dom()
        .with_id(id)
}

fn remove_button(app: &RefAny, kind: RowKind, index: usize, id: &str) -> Dom {
    Button::create("")
        .with_icon("remove_circle_outline")
        .with_on_click(RefAny::new(RowKindRef { app: app.clone(), kind, index }), on_remove_row as ButtonOnClickCallbackType)
        .dom()
        .with_id(id)
}

fn add_button(app: &RefAny, kind: RowKind, label: &str, id: &str) -> Dom {
    Button::create(label)
        .with_icon("add")
        .with_on_click(RefAny::new(RowKindRef { app: app.clone(), kind, index: 0 }), on_add_row as ButtonOnClickCallbackType)
        .dom()
        .with_id(id)
}

fn form_section(title: &str, children: Vec<Dom>) -> Dom {
    let mut all = vec![block("padding: 10px 0px 4px 0px; font-size: 11px; font-weight: 700; opacity: 0.7;", text(title.to_uppercase()))];
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

fn edit_view(app: &RefAny, form: &Form) -> Dom {
    let d = &form.draft;
    let mut children = Vec::new();
    children.push(row(
        "padding: 10px 0px;",
        vec![
            block("font-size: 18px; font-weight: 600; flex-grow: 1;", text(if form.original.is_some() { "Edit contact" } else { "New contact" })),
            button("Cancel", "edit-cancel", app, on_edit_cancel),
            primary("Save", "edit-save", app, on_edit_save),
        ],
    ));
    let mut photo_row = vec![
        Avatar::create(book::initials(d)).with_size(AvatarSize::Medium).dom(),
        button("Change photo\u{2026}", "edit-photo", app, on_photo_choose),
    ];
    if !d.photo.trim().is_empty() {
        photo_row.push(button("Remove photo", "edit-photo-remove", app, on_photo_remove));
        photo_row.push(block("font-size: 12px; opacity: 0.75;", text("A photo is set.")));
    }
    children.push(row("gap: 8px; padding-bottom: 6px;", photo_row));
    if form.confirm_discard {
        children.push(row(
            "padding: 6px 0px;",
            vec![
                block("padding-right: 8px;", text("Discard your changes?")),
                Button::create("Discard")
                    .with_button_type(ButtonType::Danger)
                    .with_on_click(app.clone(), on_edit_discard as ButtonOnClickCallbackType)
                    .dom()
                    .with_id("edit-discard"),
                button("Keep editing", "edit-keep", app, on_edit_keep),
            ],
        ));
    }
    if !form.problems.is_empty() {
        children.push(
            column("padding: 6px 8px; border-left: 3px solid #c0392b;", form.problems.iter().map(|p| Dom::create_div().with_child(text(p.as_str()))).collect())
                .with_id("edit-problems"),
        );
    }
    let pair = |a: Dom, b: Dom| row("gap: 6px; padding: 2px 0px;", vec![block("flex-grow: 1;", a), block("flex-grow: 1;", b)]);
    children.push(form_section(
        "Name",
        vec![
            pair(
                input(app, FormField::Given, &d.given, "First name", "edit-given"),
                input(app, FormField::Family, &d.family, "Last name", "edit-family"),
            ),
            pair(
                input(app, FormField::Org, &d.org, "Company", "edit-org"),
                input(app, FormField::Department, &d.department, "Department", "edit-department"),
            ),
            pair(
                input(app, FormField::Title, &d.title, "Job title", "edit-title"),
                input(app, FormField::Nickname, &d.nickname, "Nickname", "edit-nickname"),
            ),
        ],
    ));
    let mut phones: Vec<Dom> = d
        .phones
        .iter()
        .enumerate()
        .map(|(i, p)| {
            row(
                "gap: 6px; padding: 2px 0px;",
                vec![
                    label_drop(app, RowKind::Phone, i, &PHONE_LABELS, &p.label, &format!("edit-phone-label-{i}")),
                    block("flex-grow: 1;", input(app, FormField::Phone(i), &p.value, "Phone", &format!("edit-phone-{i}"))),
                    remove_button(app, RowKind::Phone, i, &format!("edit-phone-remove-{i}")),
                ],
            )
        })
        .collect();
    phones.push(add_button(app, RowKind::Phone, "Add phone", "edit-add-phone"));
    children.push(form_section("Phone", phones));
    let mut emails: Vec<Dom> = d
        .emails
        .iter()
        .enumerate()
        .map(|(i, e)| {
            row(
                "gap: 6px; padding: 2px 0px;",
                vec![
                    label_drop(app, RowKind::Email, i, &EMAIL_LABELS, &e.label, &format!("edit-email-label-{i}")),
                    block("flex-grow: 1;", input(app, FormField::Email(i), &e.value, "Email", &format!("edit-email-{i}"))),
                    remove_button(app, RowKind::Email, i, &format!("edit-email-remove-{i}")),
                ],
            )
        })
        .collect();
    emails.push(add_button(app, RowKind::Email, "Add email", "edit-add-email"));
    children.push(form_section("Email", emails));
    let mut addresses: Vec<Dom> = Vec::new();
    for (i, a) in d.addresses.iter().enumerate() {
        addresses.push(row(
            "gap: 6px; padding: 2px 0px;",
            vec![
                label_drop(app, RowKind::Address, i, &ADDRESS_LABELS, &a.label, &format!("edit-address-label-{i}")),
                block("flex-grow: 1;", input(app, FormField::Street(i), &a.street, "Street", &format!("edit-street-{i}"))),
                remove_button(app, RowKind::Address, i, &format!("edit-address-remove-{i}")),
            ],
        ));
        addresses.push(row(
            "gap: 6px; padding: 2px 0px 6px 0px;",
            vec![
                block("width: 90px;", input(app, FormField::Postcode(i), &a.postcode, "Postcode", &format!("edit-postcode-{i}"))),
                block("flex-grow: 1;", input(app, FormField::City(i), &a.locality, "City", &format!("edit-city-{i}"))),
                block("flex-grow: 1;", input(app, FormField::Region(i), &a.region, "Region", &format!("edit-region-{i}"))),
                block("flex-grow: 1;", input(app, FormField::Country(i), &a.country, "Country", &format!("edit-country-{i}"))),
            ],
        ));
    }
    addresses.push(add_button(app, RowKind::Address, "Add address", "edit-add-address"));
    children.push(form_section("Address", addresses));
    children.push(form_section(
        "Birthday",
        vec![
            input(app, FormField::Birthday, &form.birthday_text, "DD.MM.YYYY, or DD.MM. without a year", "edit-birthday"),
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
        "Groups",
        vec![
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; flex-wrap: wrap; gap: 4px; padding-bottom: 4px;")
                .with_children(DomVec::from_vec(chips)),
            row(
                "gap: 6px;",
                vec![
                    block("flex-grow: 1;", input(app, FormField::NewGroup, &form.new_group, "Add to a group", "edit-new-group")),
                    add_button(app, RowKind::Group, "Add", "edit-add-group"),
                ],
            ),
        ],
    ));
    let mut custom: Vec<Dom> = d
        .custom
        .iter()
        .enumerate()
        .map(|(i, f)| {
            row(
                "gap: 6px; padding: 2px 0px;",
                vec![
                    block("width: 140px;", input(app, FormField::CustomLabel(i), &f.label, "Field name", &format!("edit-field-label-{i}"))),
                    block("flex-grow: 1;", input(app, FormField::CustomValue(i), &f.value, "Value", &format!("edit-field-{i}"))),
                    remove_button(app, RowKind::Custom, i, &format!("edit-field-remove-{i}")),
                ],
            )
        })
        .collect();
    custom.push(add_button(app, RowKind::Custom, "Add field", "edit-add-field"));
    children.push(form_section("More fields", custom));
    children.push(form_section(
        "Notes",
        vec![TextArea::create()
            .with_text(d.notes.as_str())
            .with_placeholder("Notes")
            .with_accessibility_name("Notes")
            .with_on_text_input(app.clone(), on_notes as TextAreaOnTextInputCallbackType)
            .dom()
            .with_id("edit-notes")],
    ));
    children.push(row(
        "padding: 8px 0px;",
        vec![
            Switch::create(d.favorite)
                .with_accessibility_name("Favourite")
                .with_on_toggle(app.clone(), on_form_favorite as SwitchOnToggleCallbackType)
                .dom()
                .with_id("edit-favorite"),
            block("padding-left: 8px;", text("Favourite")),
        ],
    ));
    column("padding: 0px 16px 16px 16px; overflow-y: auto; flex-grow: 1; min-height: 0px;", children).with_id("contact-edit")
}

// ==== Import ====

struct ImportRowRef {
    app: RefAny,
    index: usize,
}

fn status_text(status: &ImportStatus, book: &[Contact]) -> String {
    match status {
        ImportStatus::New => "new".to_string(),
        ImportStatus::Update(i) => format!("updates {}", book.get(*i).map(Contact::display_name).unwrap_or_default()),
        ImportStatus::Duplicate(i, score) => format!(
            "duplicate of {} ({:.2})",
            book.get(*i).map(Contact::display_name).unwrap_or_default(),
            score
        ),
    }
}

fn import_view(s: &ContactsApp, app: &RefAny, st: &ImportState) -> Dom {
    let mut children = vec![
        block("font-size: 18px; font-weight: 600; padding: 10px 0px;", text("Import contacts")),
        row(
            "gap: 6px;",
            vec![
                block("flex-grow: 1;", input(app, FormField::ImportPath, &st.path, "Path to a .vcf file", "import-path")),
                button("Read", "import-read", app, on_import_read),
                button("Choose file\u{2026}", "import-choose", app, on_import_choose),
            ],
        ),
    ];
    if st.reading {
        children.push(block("padding: 8px 0px;", text("Reading\u{2026}")));
    }
    for p in &st.problems {
        children.push(block("font-size: 12px; opacity: 0.8;", text(p.as_str())));
    }
    if !st.rows.is_empty() {
        children.push(block("padding: 8px 0px; font-weight: 600;", text(store::import_summary(&st.rows))).with_id("import-summary"));
        let mut table = Vec::new();
        for (i, r) in st.rows.iter().enumerate() {
            let c = &r.contact;
            table.push(row(
                "gap: 8px; padding: 2px 0px; font-size: 13px;",
                vec![
                    CheckBox::create(r.selected)
                        .with_accessibility_name(format!("Import {}", c.display_name()))
                        .with_on_toggle(RefAny::new(ImportRowRef { app: app.clone(), index: i }), on_import_toggle as CheckBoxOnToggleCallbackType)
                        .dom()
                        .with_id(format!("import-row-{i}")),
                    block("width: 180px;", text(c.display_name())),
                    block("width: 200px; opacity: 0.8;", text(c.emails.first().map(|e| e.value.clone()).unwrap_or_else(|| "\u{2014}".into()))),
                    block("width: 150px; opacity: 0.8;", text(c.phones.first().map(|p| p.value.clone()).unwrap_or_else(|| "\u{2014}".into()))),
                    block("flex-grow: 1; opacity: 0.8;", text(status_text(&r.status, &s.book))),
                ],
            ));
        }
        children.push(column("", table).with_id("import-rows"));
        children.push(row(
            "gap: 6px; padding-top: 10px;",
            vec![
                block("", text("Add to group")),
                block("flex-grow: 1;", input(app, FormField::ImportGroup, &st.group, "Group (optional)", "import-group")),
                button("Cancel", "import-cancel", app, on_import_cancel),
                primary("Import", "import-run", app, on_import_run),
            ],
        ));
    } else if !st.reading {
        children.push(block(
            "padding-top: 12px; opacity: 0.75; font-size: 13px;",
            text("vCard 3.0 and 4.0 files with one or many cards. Nothing is imported before you press Import."),
        ));
        children.push(row("padding-top: 8px;", vec![button("Cancel", "import-cancel", app, on_import_cancel)]));
    }
    column("padding: 0px 16px 16px 16px; overflow-y: auto; flex-grow: 1; min-height: 0px;", children).with_id("contact-import")
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

fn merge_view(s: &ContactsApp, app: &RefAny, st: &MergeState) -> Dom {
    let Some(pair) = st.pairs.get(st.index) else {
        return ShellEmptyState::create("No possible duplicates")
            .with_icon("merge")
            .with_detail("No two contacts share a name, an email address or a phone number.")
            .dom()
            .with_id("merge-empty");
    };
    let (a, b) = (&s.book[pair.a], &s.book[pair.b]);
    let pick_row = |label: &str, field: MergeField, pick: Pick, left: String, right: String, id: &str| {
        row(
            "gap: 8px; padding: 4px 0px;",
            vec![
                block("width: 90px; font-size: 12px; opacity: 0.7;", text(label)),
                block("flex-grow: 1; font-size: 13px;", text(if left.is_empty() { "\u{2014}".to_string() } else { left })),
                Segmented::create(strs(&["Left", "Right"]))
                    .with_selected_index(usize::from(pick == Pick::B))
                    .with_on_change(RefAny::new(MergeRef { app: app.clone(), field }), on_merge_pick as SegmentedOnChangeCallbackType)
                    .dom()
                    .with_id(id),
                block("flex-grow: 1; font-size: 13px;", text(if right.is_empty() { "\u{2014}".to_string() } else { right })),
            ],
        )
    };
    let company = |c: &Contact| c.subtitle();
    let birthday = |c: &Contact| c.birthday.map(|b| b.describe()).unwrap_or_default();
    let photo = |c: &Contact| if c.photo.is_empty() { String::new() } else { "photo".to_string() };
    let list = |c: &Contact| -> Vec<String> {
        c.phones
            .iter()
            .map(|p| format!("{}: {}", p.label, p.value))
            .chain(c.emails.iter().map(|e| format!("{}: {}", e.label, e.value)))
            .chain(c.addresses.iter().map(|x| format!("{}: {}", x.label, x.lines().join(", "))))
            .collect()
    };
    let children = vec![
        row(
            "padding: 10px 0px; gap: 8px;",
            vec![
                block("font-size: 18px; font-weight: 600; flex-grow: 1;", text(format!("Possible duplicates ({})", st.pairs.len()))),
                button("\u{2039}", "merge-prev", app, on_merge_prev),
                block("font-size: 13px;", text(format!("{} of {}", st.index + 1, st.pairs.len()))).with_id("merge-position"),
                button("\u{203a}", "merge-next", app, on_merge_next),
            ],
        ),
        block(
            "font-size: 13px; padding-bottom: 8px;",
            text(format!(
                "{} \u{2194} {}   similarity {:.2} ({})",
                a.display_name(),
                b.display_name(),
                pair.score,
                pair.reasons.join(", ")
            )),
        )
        .with_id("merge-pair"),
        pick_row("Name", MergeField::Name, st.plan.name, a.display_name(), b.display_name(), "merge-name"),
        pick_row("Company", MergeField::Company, st.plan.company, company(a), company(b), "merge-company"),
        pick_row("Birthday", MergeField::Birthday, st.plan.birthday, birthday(a), birthday(b), "merge-birthday"),
        pick_row("Photo", MergeField::Photo, st.plan.photo, photo(a), photo(b), "merge-photo"),
        pick_row("Notes", MergeField::Notes, st.plan.notes, a.notes.clone(), b.notes.clone(), "merge-notes"),
        row(
            "padding: 4px 0px 4px 98px;",
            vec![
                Switch::create(st.plan.notes_both)
                    .with_accessibility_name("Keep both notes")
                    .with_on_toggle(app.clone(), on_merge_notes_both as SwitchOnToggleCallbackType)
                    .dom()
                    .with_id("merge-notes-both"),
                block("padding-left: 8px; font-size: 13px;", text("Keep both notes")),
            ],
        ),
        block("padding: 8px 0px 2px 0px; font-size: 11px; font-weight: 700; opacity: 0.7;", text("KEPT FROM BOTH")),
        row(
            "align-items: flex-start; gap: 16px;",
            vec![
                block("flex-grow: 1; font-size: 12px;", lines(list(a))),
                block("flex-grow: 1; font-size: 12px;", lines(list(b))),
            ],
        ),
        row(
            "gap: 6px; padding-top: 12px;",
            vec![
                block("flex-grow: 1;", Dom::create_div()),
                button("Not a duplicate", "merge-ignore", app, on_merge_ignore),
                primary("Merge contacts", "merge-run", app, on_merge_run),
            ],
        ),
    ];
    column("padding: 0px 16px 16px 16px; overflow-y: auto; flex-grow: 1; min-height: 0px;", children).with_id("contact-merge")
}

// ==== The panes and the window ====

fn reading_pane(s: &ContactsApp, app: &RefAny) -> Dom {
    match &s.reading {
        Reading::Edit(form) => edit_view(app, form),
        Reading::Import(st) => import_view(s, app, st),
        Reading::Merge(st) => merge_view(s, app, st),
        Reading::Card => match s.selected_index() {
            Some(i) => card_view(s, app, &s.book[i]),
            None => ShellEmptyState::create("No contact selected")
                .with_icon("person")
                .with_detail("Pick someone in the list, or add a new contact.")
                .with_action_label("New contact")
                .with_on_action(app.clone(), on_new as ButtonOnClickCallbackType)
                .dom(),
        },
    }
}

fn toolbar(app: &RefAny) -> Dom {
    let tool = |label: &str, icon: &str, id: &str, cb: ButtonOnClickCallbackType| {
        Button::create(label).with_icon(icon).with_on_click(app.clone(), cb).dom().with_id(id)
    };
    row(
        "gap: 4px; padding: 4px 8px;",
        vec![
            tool("New", "person_add", "toolbar-new", on_new),
            tool("Import", "file_upload", "toolbar-import", on_import_open),
            tool("Export", "file_download", "toolbar-export", on_export_view),
            tool("Duplicates", "merge", "toolbar-duplicates", on_open_duplicates),
            block("flex-grow: 1;", Dom::create_div()),
            tool("Settings", "settings", "toolbar-settings", on_open_settings),
        ],
    )
}

fn status_bar(s: &ContactsApp, app: &RefAny) -> Dom {
    let dupes = s.duplicates().len();
    let mut segments = vec![StatusBarSegment::create(format!(
        "{} contact{}",
        s.book.len(),
        if s.book.len() == 1 { "" } else { "s" }
    ))];
    if dupes > 0 {
        segments.push(
            StatusBarSegment::create(format!("{dupes} possible duplicate{} \u{2013} review", if dupes == 1 { "" } else { "s" }))
                .with_on_click(app.clone(), on_open_duplicates as ButtonOnClickCallbackType),
        );
    }
    if !s.notice.is_empty() {
        segments.push(StatusBarSegment::create(s.notice.as_str()));
    }
    StatusBar::create(segments).dom().with_id("contacts-status")
}

fn settings_sections(s: &ContactsApp, app: &RefAny) -> Vec<AppSection> {
    vec![AppSection {
        category: 0,
        title: "List and files".to_string(),
        content: column(
            "",
            vec![
                kit::row(
                    "Sort by",
                    Segmented::create(strs(&["First name", "Last name"]))
                        .with_selected_index(usize::from(s.sort == SortBy::Last))
                        .with_on_change(app.clone(), on_sort as SegmentedOnChangeCallbackType)
                        .dom()
                        .with_id("set-sort"),
                ),
                kit::row(
                    "Export as",
                    Segmented::create(strs(&["vCard 4.0", "vCard 3.0"]))
                        .with_selected_index(usize::from(s.export_version == Version::V3))
                        .with_on_change(app.clone(), on_export_version as SegmentedOnChangeCallbackType)
                        .dom()
                        .with_id("set-export-version"),
                ),
                kit::note(&format!(
                    "Every contact is one vCard file in {}. {} pair(s) marked as not duplicates.",
                    azul_appkit::data::local_path(&s.data_root, store::APP_FOLDER).display(),
                    s.ignored.len()
                )),
            ],
        ),
    }]
}

/// The window: the shell (or the settings page), the theme scope, the window keys.
extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let _mode = info.get_mode();
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<ContactsApp>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    let content = if kit::settings_open(&s.kit) {
        column(
            "flex-grow: 1; min-height: 0px;",
            vec![kit::title_row(SPEC.name), kit::settings_page(&s.kit, settings_sections(s, &app))],
        )
    } else {
        PimShell::create(navigation(s, &app), list_pane(s, &app), reading_pane(s, &app))
            .with_list_label("Contacts")
            .office_shell()
            .with_title_row(kit::title_row(SPEC.name))
            .with_ribbon(toolbar(&app))
            .with_status_bar(status_bar(s, &app))
            .dom()
    };
    let root = column("flex-grow: 1; min-height: 0px;", vec![content]);
    Dom::create_body()
        .with_css("display: flex; flex-direction: column; margin: 0px;")
        .with_child(ShellThemeScope::create(root).with_accent(ShellThemeAccent::Blue).dom())
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
    Update::RefreshDom
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
        s.notice = "Nothing to export.".to_string();
        return;
    }
    let text = store::export(&s.book, indices, s.export_version);
    let key = format!("exports/contacts-{}.vcf", now_secs());
    s.notice = format!(
        "Exporting {} contact{} to {}",
        indices.len(),
        if indices.len() == 1 { "" } else { "s" },
        azul_appkit::data::local_path(&s.data_root, &key).display()
    );
    write_files(s, info, app, vec![FileJob::Put { key, bytes: text.into_bytes() }], TAG_WRITE);
}

/// Reads a `.vcf` file for the import preview (on a Thread; the drive is its folder).
fn read_import_file(s: &mut ContactsApp, info: &mut CallbackInfo, app: &RefAny, path: &Path) {
    let (Some(folder), Some(name)) = (path.parent(), path.file_name()) else {
        if let Reading::Import(st) = &mut s.reading {
            st.problems = vec![format!("\"{}\" is not a file.", path.display())];
        }
        return;
    };
    let folder = if folder.as_os_str().is_empty() { Path::new(".") } else { folder };
    let mut state = match std::mem::replace(&mut s.reading, Reading::Card) {
        Reading::Import(st) => st,
        _ => ImportState {
            path: String::new(),
            rows: Vec::new(),
            problems: Vec::new(),
            group: "Imported".to_string(),
            reading: false,
        },
    };
    state.path = path.display().to_string();
    state.rows.clear();
    state.problems.clear();
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
        group: "Imported".to_string(),
        reading: false,
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
                s.notice = format!("{} contact file(s) could not be read fully", problems.len());
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
                    FileOutcome::Got { result: Ok(None), key } => problem = Some(format!("\"{key}\" does not exist.")),
                    FileOutcome::Got { result: Err(e), .. } => problem = Some(e),
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
                    let (rows, problems) = store::import_preview(&text, &book);
                    if rows.is_empty() && problems.is_empty() {
                        st.problems.push("The file holds no vCard.".to_string());
                    }
                    st.problems.extend(problems);
                    st.rows = rows;
                    println!("AZCONTACTS_IMPORT_PREVIEW {} {}", st.rows.len(), store::import_summary(&st.rows));
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
                            s.notice = format!("Exported to {}", azul_appkit::data::local_path(&s.data_root, key).display());
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
                s.notice = format!("{failed} file(s) could not be written - see the log");
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
        let node = info.get_node_id_by_id_attribute(dom, section_id(*candidate));
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
            s.notice = format!("Copied {} as vCard {}", s.book[i].display_name(), s.export_version.label());
        }
    })
}

extern "C" fn on_copy_email(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        if let Some(e) = s.selected_index().and_then(|i| s.book[i].emails.first().cloned()) {
            copy_to_clipboard(info, &e.value);
            s.notice = format!("Copied {}", e.value);
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
        s.notice = format!("Deleted {}", removed.display_name());
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
pub fn finished_draft(form: &Form) -> (Contact, Vec<String>) {
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
            println!("AZCONTACTS_PROBLEMS {}", problems.join(" | "));
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

/// Standard base64 (for a photo picked from a file).
#[must_use]
pub fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { TABLE[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[n as usize & 63] as char } else { '=' });
    }
    out
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
        "Choose a photo",
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
        s.notice = format!("Reading {}", path.display());
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
                            form.draft.photo = format!("data:{};base64,{}", image_mime(&key), base64(&bytes));
                            s.notice = "Photo set".to_string();
                        }
                    }
                    Ok(Some(_)) => s.notice = "That picture is larger than 2 MB.".to_string(),
                    Ok(None) => s.notice = format!("{key} does not exist"),
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
                st.problems = vec!["Type the path of a .vcf file, or choose one.".to_string()];
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
        "Import contacts",
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
        s.notice = format!("Imported {count} contact{}", if count == 1 { "" } else { "s" });
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
        s.notice = format!("Merged into {}", merged.display_name());
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
    fn base64_matches_the_standard_alphabet_and_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe, 0xfd]), "//79");
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
        assert_eq!(finished_draft(&empty).1, vec!["A contact needs a name or a company."]);
        assert!(!empty.changed());
    }

    #[test]
    fn the_section_ids_name_the_letters() {
        assert_eq!(section_id('A'), "section-A");
        assert_eq!(section_id('#'), "section-hash");
    }
}
