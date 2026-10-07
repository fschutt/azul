//! AzNews' window, laid out like OS X Mail (Leopard) - a news reader with selectable sources:
//!
//! - the app-drawn title row (`NoTitle` + `Titlebar`) and Mail's unified grey TOOLBAR (big icons,
//!   their labels under them: Get News, Mark Read, Star, Read Later, Open, Copy Link, Mark All
//!   Read, Add Feed, Sources; the search field at the right) - [`toolbar`];
//! - the SOURCE LIST at the left ([`sidebar`]): the "Articles" section (all, unread, starred,
//!   read later, broken feeds), then one section per source, and under each source its articles
//!   grouped by topic (the feed's own categories: a forum's subforums, a blog's categories;
//!   [`crate::library::Library::topics`]), unread counts as pills; the activity area (fetching,
//!   n of m feeds, the last word) and the +, activity and actions buttons at the bottom;
//! - the ARTICLE TABLE ([`table`]): dense rows under sortable column headers (read state, star,
//!   title, source, date; a click sorts, again turns it round), alternating, the selected row in
//!   the selection colour, Outlook's date groups (azul-pim's `DateGroup`) while it is sorted by
//!   date; a VirtualView, so only the rows in view are built;
//! - under it, on a splitter, the READING PANE: azul's `ReadingPane` with the article read
//!   through azul's HTML5-like parser in the reader stylesheet ([`crate::reader::article`]), its
//!   pictures fetched on a Thread and put into the image cache ([`crate::jobs::spawn_pictures`]);
//! - the forms take the right side while they are open: Add feed, the OPML import preview, a
//!   feed's page, the sources page ([`sources`]: which feeds are followed; add, import, export,
//!   remove).
//!
//! Everything durable is a file in the data tree ([`crate::store`]), written on an azul Thread
//! through azul-storage (`azul_appkit::ui::spawn_file_jobs`); feeds are refreshed on a Thread
//! with ETag / Last-Modified ([`crate::jobs::spawn_refresh`]).
//!
//! On stdout, for scripts/aznews_e2e.py: `AZNEWS_LOADED <feeds> <articles>`, `AZNEWS_VIEW <n>`,
//! `AZNEWS_SELECTED <feed id> <article id>`, `AZNEWS_REFRESHED <feed id> <new | 304 | error>`,
//! `AZNEWS_REFRESH_DONE <unread>`, `AZNEWS_FOUND <n>`, `AZNEWS_SUBSCRIBED <feed id> <url>`,
//! `AZNEWS_IMPORT_PREVIEW <rows>`, `AZNEWS_IMPORTED <n>`, `AZNEWS_EXPORTED <key>`,
//! `AZNEWS_PICTURE <url>`, `AZNEWS_SAVED <key>`, `AZNEWS_SAMPLE_WRITTEN <files>`,
//! `AZNEWS_UNSUBSCRIBED <feed id>`, `AZNEWS_MARKED_ALL <n>`, `AZNEWS_SORTED <column> <asc |
//! desc>`, `AZNEWS_FOLLOW <feed id> <on | off>`, `AZNEWS_COPIED <link>`.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use crate::{
    feed::Item,
    fetch::{Candidate, Fetched},
    ids,
    jobs::{self, FindEvent, PictureEvent, RefreshEvent, RefreshJob},
    library::{ArticleRef, Library, Sort, SortKey, View, DAY},
    links,
    opml::{self, Subscription},
    reader, sample, store,
};
use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, ReadingPaneOnEventCallbackType,
        SegmentedOnChangeCallbackType, ShellOnPaneResizeCallbackType,
        SplitPaneOnResizeCallbackType, SwitchOnToggleCallbackType,
        TextInputOnTextInputCallbackType, TimerCallbackInfo, TimerCallbackReturn,
    },
    dialog::{FileDialog, FileOpenResult},
    dom::ClipboardContent,
    image::ImageRef,
    option::{OptionFileTypeList, OptionString},
    prelude::*,
    shells::{
        OfficeShell, ShellEmptyState, ShellPane, ShellPaneKind, ShellThemeAccent, ShellThemeScope,
    },
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    vec::{StringVec, StyledTextRunVec},
    widgets::{
        ButtonType, CheckBox, CheckBoxState, InfoBar, OnTextInputReturn, ReadingPane,
        ReadingPaneEvent, ReadingPaneEventKind, Segmented, SegmentedState, SplitDirection,
        SplitPane, SplitPaneState, Switch, SwitchState, TextInputState, TextInputValid,
    },
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec},
    files::{FileJob, FileOutcome},
    pieces::{block, button, column, flex_row, primary, text},
    settings::AppSettings,
    shortcuts::Shortcut,
    ui::{self as kit, AppSection},
};

/// The source list (left): the "Articles" section, a section per source with its topics, the
/// activity area, the buttons under it.
mod sidebar;
/// The sources page: which feeds are followed; add, import, export, remove.
mod sources;
/// The article table: sortable headers, a VirtualView of dense rows.
mod table;
/// Mail's unified toolbar.
mod toolbar;

pub use table::{list_date, TableRow};

// ==== The app's facts ====

pub const SCREENS: [&str; 6] = ["articles", "add", "import", "feed", "sources", "settings"];

pub const SPEC: AppSpec = AppSpec {
    name: "AzNews",
    binary: "AzNews",
    summary: "a feed reader: RSS, Atom and JSON Feed, read in a clean reader view",
    screens: &SCREENS,
    files_help: ".opml files to import (subscription lists from another reader)",
};

pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzNews",
    version: env!("CARGO_PKG_VERSION"),
    summary: "Your feeds - RSS, Atom and JSON Feed - in folders, refreshed politely (ETag, \
              Last-Modified), read in a clean reader view. The list is an OPML file, the articles \
              and your marks are plain files. Part of the Azlin apps, built with azul.",
    license: "MIT",
    app_folder: store::APP_FOLDER,
};

pub const SHORTCUTS: [Shortcut; 12] = [
    Shortcut::new("Articles", "J / Down", "Next article"),
    Shortcut::new("Articles", "K / Up", "Previous article"),
    Shortcut::new("Articles", "S", "Star or unstar the article"),
    Shortcut::new("Articles", "M", "Mark the article read or unread"),
    Shortcut::new("Articles", "L", "Read later"),
    Shortcut::new("Articles", "O", "Open the original in the browser"),
    Shortcut::new("Feeds", "R", "Get news: refresh every followed feed"),
    Shortcut::new("Feeds", "Mod+N", "Add a feed"),
    Shortcut::new("Feeds", "Mod+O", "Import an OPML file"),
    Shortcut::new("Feeds", "Mod+E", "Export the subscriptions as OPML"),
    Shortcut::new("Feeds", "Escape", "Leave a form"),
    Shortcut::new("Panes", "F6 / Shift+F6", "Next / previous pane"),
];

const APP_CATEGORIES: [&str; 2] = ["Reading", "Refresh"];

const TAG_LOAD: u64 = 1;
const TAG_WRITE: u64 = 2;
const TAG_SAMPLE: u64 = 3;
const TAG_IMPORT_FILE: u64 = 4;
/// The subscription list written on the way out: the window closes when it landed.
const TAG_CLOSING: u64 = 5;

/// A library of at most this many sources opens every source's section at the start; a bigger
/// one opens a section when the user does (or picks one of its rows).
pub const OPEN_SOURCES_UP_TO: usize = 8;

/// The reading font sizes offered (px).
pub const FONT_SIZES: [u32; 5] = [16, 18, 20, 22, 24];
/// The line widths offered: (label, px).
pub const MEASURES: [(&str, u32); 3] = [("Narrow", 560), ("Medium", 680), ("Wide", 820)];
/// How often feeds are refreshed while the window is open: (label, minutes; 0 = never).
pub const REFRESH_EVERY: [(&str, u32); 4] = [
    ("Never", 0),
    ("15 min", 15),
    ("Hourly", 60),
    ("4 hours", 240),
];
/// How long an article the feed dropped is kept: (label, days).
pub const KEEP: [(&str, u32); 3] = [("A week", 7), ("A month", 30), ("Three months", 90)];

/// The DOM ids of the shell's two panes (what F6 finds them by).
const NAVIGATION_PANE: &str = "shell-navigation";
const MAIN_PANE: &str = "shell-reading";

/// A box that fills a splitter's pane (the pane itself is no flex box).
const FILL: &str = "display: flex; flex-direction: column; width: 100%; height: 100%; \
                    min-width: 0px; min-height: 0px; overflow: hidden;";

// ==== Settings ====

/// When the pictures of an article are fetched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pictures {
    Always,
    /// After "Load pictures" (per article).
    OnClick,
    Never,
}

impl Pictures {
    pub const ALL: [Pictures; 3] = [Pictures::Always, Pictures::OnClick, Pictures::Never];

    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Pictures::Always => "always",
            Pictures::OnClick => "click",
            Pictures::Never => "never",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Pictures::Always => "Always",
            Pictures::OnClick => "On click",
            Pictures::Never => "Never",
        }
    }

    #[must_use]
    pub fn parse(key: Option<&str>) -> Pictures {
        Pictures::ALL
            .into_iter()
            .find(|p| Some(p.key()) == key)
            .unwrap_or(Pictures::OnClick)
    }
}

/// The app's own settings (in `news/settings.json` through the kit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub font_px: u32,
    pub measure_px: u32,
    /// Warm paper under the article.
    pub sepia: bool,
    pub pictures: Pictures,
    pub strip_tracking: bool,
    pub refresh_on_start: bool,
    pub refresh_minutes: u32,
    pub keep_days: u32,
    /// The article table's order (the column header clicked last).
    pub sort: Sort,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            font_px: 20,
            measure_px: 680,
            sepia: false,
            pictures: Pictures::OnClick,
            strip_tracking: true,
            refresh_on_start: true,
            refresh_minutes: 60,
            keep_days: 30,
            sort: Sort::default(),
        }
    }
}

/// A number setting, kept to the values offered (`default` otherwise).
fn number_in(value: Option<&str>, offered: &[u32], default: u32) -> u32 {
    value
        .and_then(|v| v.trim().parse::<u32>().ok())
        .filter(|v| offered.contains(v))
        .unwrap_or(default)
}

/// A table order as the settings keep it: `<column>-<asc | desc>` (`date-desc`).
#[must_use]
pub fn sort_value(sort: Sort) -> String {
    format!(
        "{}-{}",
        sort.key.key(),
        if sort.descending { "desc" } else { "asc" }
    )
}

/// The table order a settings value names (newest first for anything else).
#[must_use]
pub fn parse_sort(value: Option<&str>) -> Sort {
    let Some((key, way)) = value.and_then(|v| v.trim().split_once('-')) else {
        return Sort::default();
    };
    let key = SortKey::parse(Some(key));
    Sort {
        key,
        descending: match way {
            "asc" => false,
            "desc" => true,
            _ => key.first_descending(),
        },
    }
}

impl Settings {
    /// The settings from the kit's values (each unknown value its default).
    #[must_use]
    pub fn read(get: &dyn Fn(&str) -> Option<String>) -> Settings {
        let d = Settings::default();
        let flag = |key: &str, default: bool| match get(key).as_deref() {
            Some("true") => true,
            Some("false") => false,
            _ => default,
        };
        let measures: Vec<u32> = MEASURES.iter().map(|(_, px)| *px).collect();
        let every: Vec<u32> = REFRESH_EVERY.iter().map(|(_, m)| *m).collect();
        let keep: Vec<u32> = KEEP.iter().map(|(_, d)| *d).collect();
        Settings {
            font_px: number_in(get("font").as_deref(), &FONT_SIZES, d.font_px),
            measure_px: number_in(get("measure").as_deref(), &measures, d.measure_px),
            sepia: get("paper").as_deref() == Some("sepia"),
            pictures: Pictures::parse(get("pictures").as_deref()),
            strip_tracking: flag("strip", d.strip_tracking),
            refresh_on_start: flag("refresh_start", d.refresh_on_start),
            refresh_minutes: number_in(get("refresh_every").as_deref(), &every, d.refresh_minutes),
            keep_days: number_in(get("keep").as_deref(), &keep, d.keep_days),
            sort: parse_sort(get("sort").as_deref()),
        }
    }
}

// ==== Ages and dates ====

/// How old something of `date` is at `now`, as the list says it: `now`, `5 min`, `2 h`, `3 d`,
/// else the date (`2026-08-01`).
#[must_use]
pub fn age(date: i64, now: i64) -> String {
    let secs = (now - date).max(0);
    if secs < 60 {
        "now".to_string()
    } else if secs < 3_600 {
        format!("{} min", secs / 60)
    } else if secs < DAY {
        format!("{} h", secs / 3_600)
    } else if secs < 7 * DAY {
        format!("{} d", secs / DAY)
    } else {
        chrono::DateTime::<chrono::Utc>::from_timestamp(date, 0)
            .map(|d| {
                d.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d")
                    .to_string()
            })
            .unwrap_or_default()
    }
}

/// "Updated 5 min ago", "Updated just now", "Updated 2026-08-01" (`last` = when; 0 = never).
#[must_use]
pub fn updated_line(last: i64, now: i64) -> String {
    if last <= 0 {
        return "Not refreshed yet".to_string();
    }
    let ago = age(last, now);
    if ago == "now" {
        "Updated just now".to_string()
    } else if now - last >= 7 * DAY {
        format!("Updated {ago}")
    } else {
        format!("Updated {ago} ago")
    }
}

/// An article's date for the reading pane: `Wednesday, 30 September 2026, 10:42` (local time).
#[must_use]
pub fn long_date(date: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(date, 0)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%A, %-d %B %Y, %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

/// The local offset from UTC now, in seconds (the table's dates and date groups).
fn local_offset_secs() -> i64 {
    use chrono::Offset;
    i64::from(chrono::Local::now().offset().fix().local_minus_utc())
}

fn now_secs() -> i64 {
    i64::try_from(azul_storage::time::now_unix()).unwrap_or(0)
}

// ==== State ====

/// "Add feed": what was typed, what was found.
#[derive(Debug, Clone, Default)]
pub struct AddFeed {
    pub input: String,
    pub finding: bool,
    pub problem: String,
    pub candidates: Vec<Candidate>,
    pub chosen: usize,
    pub folder: String,
}

/// One row of the OPML import preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRow {
    pub sub: Subscription,
    /// Its address is subscribed already.
    pub known: bool,
    pub selected: bool,
}

/// The OPML import preview.
#[derive(Debug, Clone, Default)]
pub struct OpmlImport {
    pub path: String,
    pub reading: bool,
    pub problem: String,
    pub rows: Vec<ImportRow>,
}

/// The rows of an import preview: every subscription of the file, the ones already subscribed
/// marked and not selected.
#[must_use]
pub fn import_rows(subs: Vec<Subscription>, library: &Library) -> Vec<ImportRow> {
    subs.into_iter()
        .map(|mut sub| {
            sub.id.clear();
            let known = library.feeds.iter().any(|f| f.sub.url == sub.url);
            ImportRow {
                sub,
                known,
                selected: !known,
            }
        })
        .collect()
}

/// What the right side shows.
#[derive(Debug, Clone)]
pub enum Reading {
    /// The article table and, under it, the selected article (or the empty state).
    Article,
    AddFeed(AddFeed),
    Import(OpmlImport),
    /// A feed's page, by its id.
    Feed(String),
    /// The sources page.
    Sources,
}

/// The app's state.
pub struct NewsApp {
    pub kit: RefAny,
    pub data_root: PathBuf,
    pub sample: bool,
    pub import_files: Vec<PathBuf>,
    pub library: Library,
    pub loaded: bool,
    pub view: View,
    pub query: String,
    pub unread_only: bool,
    /// The open article: (feed id, article id).
    pub selected: Option<(String, String)>,
    pub reading: Reading,
    /// Sources whose section the user opened (`true`) or closed (`false`), by feed id; the
    /// others follow [`OPEN_SOURCES_UP_TO`].
    pub source_open: BTreeMap<String, bool>,
    /// The "Articles" section of the source list is open.
    pub articles_open: bool,
    /// The activity area under the source list shows.
    pub show_activity: bool,
    pub notice: String,
    /// Feeds still to answer in the running refresh, and how many it asked.
    pub refreshing: usize,
    pub refresh_total: usize,
    /// When the last refresh ended.
    pub last_refresh: i64,
    /// Pictures in azul's image cache, asked for, and failed (by address).
    pub pictures: BTreeSet<String>,
    pub pictures_asked: BTreeSet<String>,
    pub pictures_failed: BTreeSet<String>,
    /// Articles whose pictures the user asked for ("Load pictures"), by article id.
    pub pictures_allowed: BTreeSet<String>,
    pub settings: Settings,
    pub start_screen: String,
    /// "Mark all as read?" is asked.
    pub confirm_mark_all: bool,
    /// "Unsubscribe?" is asked on the feed page.
    pub confirm_unsubscribe: bool,
    /// "Remove?" is asked on the sources page, for this feed id.
    pub confirm_remove: Option<String>,
    /// The feed page changed the subscription list (written when the page is left, or when
    /// the window is asked to close).
    pub list_dirty: bool,
    /// The window waits for the subscription list to land before it closes.
    pub closing: bool,
    /// The "Refresh every ..." timer, while one runs.
    pub refresh_timer: Option<TimerId>,
    /// The source list's share of the window's width.
    pub nav_ratio: f32,
    /// The article table's share of the height above the reading pane.
    pub split_ratio: f32,
}

impl NewsApp {
    fn new(kit_ref: RefAny, args: &AppArgs) -> NewsApp {
        let mut k = kit_ref.clone();
        let (data_root, settings) = match k.downcast_ref::<kit::Kit>() {
            Some(kit) => (
                kit.data_root.clone(),
                Settings::read(&|key| kit.settings.get(key).map(str::to_string)),
            ),
            None => (PathBuf::from("."), Settings::default()),
        };
        NewsApp {
            kit: kit_ref,
            data_root,
            sample: args.sample,
            import_files: args.files.clone(),
            library: Library::default(),
            loaded: false,
            view: View::All,
            query: String::new(),
            unread_only: false,
            selected: None,
            reading: Reading::Article,
            source_open: BTreeMap::new(),
            articles_open: true,
            show_activity: true,
            notice: String::new(),
            refreshing: 0,
            refresh_total: 0,
            last_refresh: 0,
            pictures: BTreeSet::new(),
            pictures_asked: BTreeSet::new(),
            pictures_failed: BTreeSet::new(),
            pictures_allowed: BTreeSet::new(),
            settings,
            start_screen: args.screen.clone().unwrap_or_default(),
            confirm_mark_all: false,
            confirm_unsubscribe: false,
            confirm_remove: None,
            list_dirty: false,
            closing: false,
            refresh_timer: None,
            nav_ratio: 0.2,
            split_ratio: 0.4,
        }
    }

    /// The list: the view's articles that match the search (and are unread, with "Unread"),
    /// newest first.
    fn list(&self) -> Vec<ArticleRef> {
        let view = if self.unread_only && self.view == View::All {
            View::Unread
        } else {
            self.view.clone()
        };
        let mut list = self.library.list(&view, &self.query);
        if self.unread_only && view != View::Unread {
            list.retain(|r| !self.library.is_read(*r));
        }
        list
    }

    /// The list in the table's order (the column clicked last).
    fn ordered(&self) -> Vec<ArticleRef> {
        let mut list = self.list();
        self.library.sort(&mut list, self.settings.sort);
        list
    }

    /// The table's rows: [`Self::ordered`], under their days' headers when sorted by date.
    fn table_rows(&self) -> Vec<TableRow> {
        let today = chrono::Local::now().date_naive();
        table::rows(
            &self.library,
            &self.ordered(),
            self.settings.sort,
            today,
            local_offset_secs(),
        )
    }

    /// The open article.
    fn selected_ref(&self) -> Option<ArticleRef> {
        let (feed_id, item_id) = self.selected.as_ref()?;
        let feed = self.library.feed_index(feed_id)?;
        let item = self.library.feeds[feed]
            .items
            .iter()
            .position(|i| &i.id == item_id)?;
        Some(ArticleRef { feed, item })
    }

    fn reference_of(&self, r: ArticleRef) -> Option<(String, String)> {
        let item = self.library.article(r)?;
        Some((self.library.feeds[r.feed].sub.id.clone(), item.id.clone()))
    }

    /// The open article's link, without its tracking parameters when the setting says so
    /// (`None`: no article, or one without a link).
    fn selected_link(&self) -> Option<String> {
        let link = self
            .selected_ref()
            .and_then(|r| self.library.article(r))
            .map(|i| i.link.trim().to_string())
            .filter(|l| !l.is_empty())?;
        Some(if self.settings.strip_tracking {
            links::strip_tracking(&link)
        } else {
            link
        })
    }

    /// Whether the source's section in the source list is open.
    fn source_is_open(&self, id: &str) -> bool {
        self.source_open
            .get(id)
            .copied()
            .unwrap_or(self.library.feeds.len() <= OPEN_SOURCES_UP_TO)
    }

    /// Whether the article's pictures are fetched (the setting, or "Load pictures").
    fn pictures_on(&self, item: &Item) -> bool {
        match self.settings.pictures {
            Pictures::Always => true,
            Pictures::OnClick => self.pictures_allowed.contains(&item.id),
            Pictures::Never => false,
        }
    }

    /// The reader view of an article with the current settings.
    fn article_view(&self, item: &Item) -> reader::Article {
        let css = reader::reader_css(
            self.settings.font_px,
            self.settings.measure_px,
            self.settings.sepia,
        );
        reader::article(
            item.body(),
            &item.base,
            self.pictures_on(item),
            self.settings.strip_tracking,
            &css,
        )
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
    let app = NewsApp::new(kit_ref.clone(), &args);
    let config = kit::app_config(&kit_ref);
    let window = kit::window_options(
        &kit_ref,
        layout,
        (1200.0, 760.0),
        (720.0, 460.0),
        on_window_created,
    );
    App::create(RefAny::new(app), config).run(window);
}

// ==== Small pieces ====

// `text`, `block`, `column`, `flex_row`, `button` and `primary` are the shared
// azul_appkit::pieces (imported above); only the text field is AzNews' own.

fn input(
    value: &str,
    placeholder: &str,
    id: AzString,
    app: &RefAny,
    cb: TextInputOnTextInputCallbackType,
) -> Dom {
    TextInput::create()
        .with_text(value)
        .with_placeholder(placeholder)
        .with_on_text_input(app.clone(), cb)
        .dom()
        .with_id(id)
}

/// The view's name: the table's heading.
fn view_title(s: &NewsApp) -> String {
    let feed_name = |id: &str| {
        s.library
            .feed_index(id)
            .map_or_else(String::new, |i| s.library.feeds[i].name().to_string())
    };
    match &s.view {
        View::All => "All articles".to_string(),
        View::Unread => "Unread".to_string(),
        View::Starred => "Starred".to_string(),
        View::Later => "Read later".to_string(),
        View::Broken => "Broken feeds".to_string(),
        View::Folder(name) => name.clone(),
        View::Feed(id) => feed_name(id.as_str()),
        View::Topic(id, topic) => format!("{} \u{203a} {topic}", feed_name(id.as_str())),
    }
}

/// An article's title for the table and the pane: its own, else the start of its text.
fn title_of(item: &Item) -> String {
    if item.title.trim().is_empty() {
        let start = reader::cut_at_word(&item.excerpt, 80);
        if start.is_empty() {
            "(no title)".to_string()
        } else {
            start
        }
    } else {
        item.title.clone()
    }
}

// ==== The reading pane ====

fn empty_reading(app: &RefAny) -> Dom {
    ShellEmptyState::create("No article selected")
        .with_icon("article")
        .with_detail("Pick an article in the list, or add a feed.")
        .with_action_label("Add feed")
        .with_on_action(app.clone(), on_add_open as ButtonOnClickCallbackType)
        .dom()
}

fn article_view(s: &NewsApp, app: &RefAny, r: ArticleRef) -> Dom {
    let lib = &s.library;
    let feed = &lib.feeds[r.feed];
    let item = &feed.items[r.item];
    let article = s.article_view(item);
    let title = title_of(item);
    let mut pane = ReadingPane::create(title.as_str(), feed.name())
        .with_date(long_date(item.date()))
        .with_field(
            "Reading time",
            format!("{} min", reader::reading_minutes(article.words)),
        );
    if !item.author.is_empty() {
        pane = pane.with_field("Author", item.author.as_str());
    }
    if !item.link.is_empty() {
        pane = pane.with_field("Link", item.link.as_str());
    }
    if article.blocked > 0 {
        let notice = match s.settings.pictures {
            Pictures::Never => InfoBar::create(format!(
                "{} picture(s) are not shown (Settings, Reading, Pictures).",
                article.blocked
            ))
            .with_icon("info"),
            _ => InfoBar::create(format!(
                "{} picture(s) were not loaded: the sites they are on learn nothing of what you read.",
                article.blocked
            ))
            .with_icon("info")
            .with_action("Load pictures"),
        };
        pane = pane.with_info_bar(notice);
    }
    let body = Dom::create_div()
        .with_css("overflow-x: auto;")
        .with_child(Dom::create_from_parsed_xml(article.xml))
        .with_id(ids::READER);
    let pane = pane
        .with_body(body)
        .with_on_load_images(
            app.clone(),
            on_reading_event as ReadingPaneOnEventCallbackType,
        )
        .with_on_link(
            app.clone(),
            on_reading_event as ReadingPaneOnEventCallbackType,
        )
        .dom();
    block("flex-grow: 1; min-height: 0px; overflow-y: auto;", pane)
}

/// The reading pane under the table: the open article, or the empty state.
fn reading_view(s: &NewsApp, app: &RefAny) -> Dom {
    let content = match s.selected_ref() {
        Some(r) => article_view(s, app, r),
        None => empty_reading(app),
    };
    column(FILL, vec![content])
}

/// A form's section title.
fn section_title(title: &str) -> Dom {
    block(
        "padding: 12px 0px 4px 0px; font-size: 11px; font-weight: 700; opacity: 0.7;",
        text(title.to_uppercase()),
    )
}

fn problem_line(problem: &str, id: AzString) -> Dom {
    block(
        "color: #b3261e; padding: 4px 0px; font-size: 13px; \
         @media (prefers-color-scheme: dark) { color: #f2b8b5; }",
        text(problem),
    )
    .with_id(id)
}

/// A form's scrolling page on the right side.
fn form_page(children: Vec<Dom>) -> Dom {
    column(
        "padding: 16px; flex-grow: 1; min-height: 0px; overflow-y: auto;",
        children,
    )
}

struct PickRef {
    app: RefAny,
    index: usize,
}

fn add_feed_view(app: &RefAny, st: &AddFeed) -> Dom {
    let mut children = vec![
        block(
            "font-size: 18px; font-weight: 600; padding-bottom: 8px;",
            text("Add a feed"),
        ),
        kit::row(
            "Website or feed",
            flex_row(
                "gap: 6px; flex-grow: 1;",
                vec![
                    block(
                        "flex-grow: 1;",
                        input(
                            &st.input,
                            "https://example.org",
                            ids::ADD_URL,
                            app,
                            on_add_input,
                        ),
                    ),
                    primary("Find", ids::ADD_FIND, app, on_add_find),
                ],
            ),
        ),
    ];
    if st.finding {
        children.push(kit::note("Looking for feeds\u{2026}"));
    }
    if !st.problem.is_empty() {
        children.push(problem_line(&st.problem, ids::ADD_PROBLEM));
    }
    let mut actions = Vec::new();
    if !st.candidates.is_empty() {
        children.push(section_title(&format!(
            "Found {} feed{}",
            st.candidates.len(),
            if st.candidates.len() == 1 { "" } else { "s" }
        )));
        for (i, c) in st.candidates.iter().enumerate() {
            let chosen = i == st.chosen;
            let name = if !c.feed.title.is_empty() {
                c.feed.title.clone()
            } else if !c.link_title.is_empty() {
                c.link_title.clone()
            } else {
                c.url.clone()
            };
            let label = format!(
                "{name} \u{b7} {} \u{b7} {} articles",
                c.feed.format.label(),
                c.feed.items.len()
            );
            let mut pick = Button::create(label);
            if chosen {
                pick = pick.with_button_type(ButtonType::Primary);
            }
            children.push(block(
                "padding: 2px 0px;",
                pick.with_on_click(
                    RefAny::new(PickRef {
                        app: app.clone(),
                        index: i,
                    }),
                    on_add_pick as ButtonOnClickCallbackType,
                )
                .dom()
                .with_id(ids::found(i)),
            ));
            if chosen {
                for item in c.feed.items.iter().take(3) {
                    children.push(block(
                        "padding-left: 16px; font-size: 12px; opacity: 0.8;",
                        text(title_of(item)),
                    ));
                }
            }
        }
        children.push(kit::row(
            "Folder",
            input(&st.folder, "No folder", ids::ADD_FOLDER, app, on_add_folder),
        ));
        actions.push(primary(
            "Subscribe",
            ids::ADD_SUBSCRIBE,
            app,
            on_add_subscribe,
        ));
    }
    actions.push(button("Cancel", ids::ADD_CANCEL, app, on_leave));
    children.push(flex_row("gap: 6px; padding-top: 12px;", actions));
    form_page(children).with_id(ids::ADD_FEED)
}

struct ImportRowRef {
    app: RefAny,
    index: usize,
}

fn import_view(app: &RefAny, st: &OpmlImport) -> Dom {
    let mut children = vec![
        block(
            "font-size: 18px; font-weight: 600; padding-bottom: 8px;",
            text("Import subscriptions (OPML)"),
        ),
        kit::row(
            "File",
            flex_row(
                "gap: 6px; flex-grow: 1;",
                vec![
                    block(
                        "flex-grow: 1;",
                        input(
                            &st.path,
                            "/path/to/subscriptions.opml",
                            ids::OPML_PATH,
                            app,
                            on_import_path,
                        ),
                    ),
                    button("Choose\u{2026}", ids::OPML_CHOOSE, app, on_import_choose),
                    button("Read", ids::OPML_READ, app, on_import_read),
                ],
            ),
        ),
    ];
    if st.reading {
        children.push(kit::note("Reading the file\u{2026}"));
    }
    if !st.problem.is_empty() {
        children.push(problem_line(&st.problem, ids::OPML_SUMMARY));
    }
    if !st.rows.is_empty() {
        let new = st.rows.iter().filter(|r| !r.known).count();
        children.push(
            section_title(&format!(
                "{} feeds, {new} new, {} subscribed already",
                st.rows.len(),
                st.rows.len() - new
            ))
            .with_id(ids::OPML_SUMMARY),
        );
        let mut rows = Dom::create_div()
            .with_id(ids::OPML_ROWS)
            .with_css("display: flex; flex-direction: column;");
        for (i, r) in st.rows.iter().enumerate() {
            let check = CheckBox::create(r.selected)
                .with_on_toggle(
                    RefAny::new(ImportRowRef {
                        app: app.clone(),
                        index: i,
                    }),
                    on_import_toggle as CheckBoxOnToggleCallbackType,
                )
                .dom();
            let folder = if r.sub.folder.is_empty() {
                String::new()
            } else {
                format!(" \u{b7} {}", r.sub.folder)
            };
            rows.add_child(
                flex_row(
                    "gap: 8px; padding: 3px 0px; font-size: 13px;",
                    vec![
                        check,
                        column(
                            "flex-grow: 1; min-width: 0px;",
                            vec![
                                block("", text(format!("{}{folder}", r.sub.title))),
                                block("font-size: 11px; opacity: 0.7;", text(r.sub.url.as_str())),
                            ],
                        ),
                        block(
                            "font-size: 11px; opacity: 0.7;",
                            text(if r.known { "subscribed" } else { "new" }),
                        ),
                    ],
                )
                .with_id(ids::opml_row(i)),
            );
        }
        children.push(rows);
    }
    let mut actions = Vec::new();
    if st.rows.iter().any(|r| r.selected) {
        actions.push(primary("Import", ids::OPML_RUN, app, on_import_run));
    }
    actions.push(button("Cancel", ids::OPML_CANCEL, app, on_leave));
    children.push(flex_row("gap: 6px; padding-top: 12px;", actions));
    form_page(children).with_id(ids::OPML_IMPORT)
}

fn feed_page(s: &NewsApp, app: &RefAny, index: usize) -> Dom {
    let f = &s.library.feeds[index];
    let checked = if f.meta.checked == 0 {
        "never".to_string()
    } else {
        format!("{} ago", age(f.meta.checked, now_secs()))
    };
    let mut children = vec![
        block(
            "font-size: 18px; font-weight: 600; padding-bottom: 8px;",
            text(f.name()),
        ),
        kit::row(
            "Name",
            input(
                &f.sub.title,
                "The feed's own title",
                ids::FEED_TITLE,
                app,
                on_feed_title,
            ),
        ),
        kit::row(
            "Folder",
            input(
                &f.sub.folder,
                "No folder",
                ids::FEED_FOLDER,
                app,
                on_feed_folder,
            ),
        ),
        kit::row("Address", text(f.sub.url.as_str())),
        kit::row("Website", text(f.meta.site.as_str())),
        kit::row("Format", text(f.meta.kind.as_str())),
        kit::row(
            "Articles",
            text(format!(
                "{} ({} unread)",
                f.items.len(),
                s.library.unread(index)
            )),
        ),
        kit::row(
            "Followed",
            text(if f.sub.paused {
                "no - not refreshed, not in All articles (Sources)"
            } else {
                "yes"
            }),
        ),
        kit::row("Last asked", text(checked)),
    ];
    if !f.meta.error.is_empty() {
        children.push(problem_line(
            &format!("The last refresh failed: {}", f.meta.error),
            ids::ADD_PROBLEM,
        ));
    }
    let mut actions = vec![
        primary("Refresh now", ids::FEED_REFRESH, app, on_feed_refresh),
        button("Show its articles", ids::FEED_PAGE, app, on_feed_articles),
    ];
    if s.confirm_unsubscribe {
        actions.push(
            Button::create("Unsubscribe and delete its articles")
                .with_button_type(ButtonType::Danger)
                .with_on_click(
                    app.clone(),
                    on_unsubscribe_confirmed as ButtonOnClickCallbackType,
                )
                .dom()
                .with_id(ids::FEED_UNSUBSCRIBE_CONFIRM),
        );
    } else {
        actions.push(button(
            "Unsubscribe",
            ids::FEED_UNSUBSCRIBE,
            app,
            on_unsubscribe,
        ));
    }
    actions.push(button("Done", ids::FEED_DONE, app, on_leave));
    children.push(flex_row(
        "gap: 6px; padding-top: 12px; flex-wrap: wrap;",
        actions,
    ));
    form_page(children)
}

/// The right side: the table over the reading pane (on a splitter), or the open form.
fn main_pane(s: &NewsApp, app: &RefAny) -> Dom {
    match &s.reading {
        Reading::Article => SplitPane::create(
            SplitDirection::Vertical,
            table::list_pane(s, app),
            reading_view(s, app),
        )
        .with_ratio(s.split_ratio)
        .with_on_resize(app.clone(), on_split_resize as SplitPaneOnResizeCallbackType)
        .dom(),
        Reading::AddFeed(st) => add_feed_view(app, st),
        Reading::Import(st) => import_view(app, st),
        Reading::Feed(id) => match s.library.feed_index(id) {
            Some(i) => feed_page(s, app, i),
            None => empty_reading(app),
        },
        Reading::Sources => sources::sources_page(s, app),
    }
}

// ==== Settings, the window ====

/// A settings choice of several labels.
fn choice(
    labels: Vec<String>,
    selected: usize,
    app: &RefAny,
    cb: SegmentedOnChangeCallbackType,
    id: AzString,
) -> Dom {
    Segmented::create(StringVec::from_vec(
        labels.into_iter().map(AzString::from).collect(),
    ))
    .with_selected_index(selected)
    .with_on_change(app.clone(), cb)
    .dom()
    .with_id(id)
}

fn switch(on: bool, app: &RefAny, cb: SwitchOnToggleCallbackType, id: AzString) -> Dom {
    Switch::create(on)
        .with_on_toggle(app.clone(), cb)
        .dom()
        .with_id(id)
}

fn settings_sections(s: &NewsApp, app: &RefAny) -> Vec<AppSection> {
    let st = &s.settings;
    let font = FONT_SIZES
        .iter()
        .position(|p| *p == st.font_px)
        .unwrap_or(2);
    let measure = MEASURES
        .iter()
        .position(|(_, px)| *px == st.measure_px)
        .unwrap_or(1);
    let pictures = Pictures::ALL
        .iter()
        .position(|p| *p == st.pictures)
        .unwrap_or(1);
    let every = REFRESH_EVERY
        .iter()
        .position(|(_, m)| *m == st.refresh_minutes)
        .unwrap_or(2);
    let keep = KEEP
        .iter()
        .position(|(_, d)| *d == st.keep_days)
        .unwrap_or(1);
    vec![
        AppSection {
            category: 0,
            title: "Reading".to_string(),
            content: column(
                "",
                vec![
                    kit::row(
                        "Font size",
                        choice(
                            FONT_SIZES.iter().map(|p| format!("{p} px")).collect(),
                            font,
                            app,
                            on_set_font,
                            ids::SET_FONT_SIZE,
                        ),
                    ),
                    kit::row(
                        "Line width",
                        choice(
                            MEASURES.iter().map(|(l, _)| (*l).to_string()).collect(),
                            measure,
                            app,
                            on_set_measure,
                            ids::SET_LINE_WIDTH,
                        ),
                    ),
                    kit::row(
                        "Paper",
                        choice(
                            vec!["Follow the theme".to_string(), "Sepia".to_string()],
                            usize::from(st.sepia),
                            app,
                            on_set_paper,
                            ids::SET_PAPER,
                        ),
                    ),
                    kit::row(
                        "Pictures",
                        choice(
                            Pictures::ALL.iter().map(|p| p.label().to_string()).collect(),
                            pictures,
                            app,
                            on_set_pictures,
                            ids::SET_IMAGES,
                        ),
                    ),
                    kit::row(
                        "Strip tracking",
                        switch(st.strip_tracking, app, on_set_strip, ids::SET_STRIP_TRACKING),
                    ),
                    kit::note(
                        "Articles are set in a serif at the size chosen; their colours follow the app's theme and \
                         its light or dark mode. Tracking parameters (utm_*, fbclid, ...) come off links.",
                    ),
                ],
            ),
        },
        AppSection {
            category: 1,
            title: "Refresh".to_string(),
            content: column(
                "",
                vec![
                    kit::row(
                        "Refresh on start",
                        switch(st.refresh_on_start, app, on_set_refresh_start, ids::SET_REFRESH_START),
                    ),
                    kit::row(
                        "Refresh every",
                        choice(
                            REFRESH_EVERY.iter().map(|(l, _)| (*l).to_string()).collect(),
                            every,
                            app,
                            on_set_refresh_every,
                            ids::SET_REFRESH_EVERY,
                        ),
                    ),
                    kit::row(
                        "Keep articles",
                        choice(
                            KEEP.iter().map(|(l, _)| (*l).to_string()).collect(),
                            keep,
                            app,
                            on_set_keep,
                            ids::SET_KEEP_DAYS,
                        ),
                    ),
                    kit::note(&format!(
                        "A refresh asks each followed feed with what it said last time (ETag, Last-Modified): \
                         a feed without news costs nothing. Your feeds, articles and marks are files in {}.",
                        azul_appkit::data::local_path(&s.data_root, store::APP_FOLDER).display()
                    )),
                ],
            ),
        },
    ]
}

/// The window: the shell (or the settings page), the theme scope, the window keys.
extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let _mode = info.get_mode();
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<NewsApp>() else {
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
        OfficeShell::create()
            .with_title_row(kit::title_row(SPEC.name))
            .with_ribbon(toolbar::toolbar(s, &app))
            .with_pane(
                ShellPane::create(NAVIGATION_PANE, sidebar::sidebar(s, &app))
                    .with_kind(ShellPaneKind::Navigation)
                    .with_label("Sources")
                    .with_ratio(s.nav_ratio),
            )
            .with_pane(
                ShellPane::create(MAIN_PANE, main_pane(s, &app))
                    .with_kind(ShellPaneKind::Main)
                    .with_label("Articles"),
            )
            .with_on_pane_resize(app.clone(), on_pane_resize as ShellOnPaneResizeCallbackType)
            .dom()
    };
    let root = column("flex-grow: 1; min-height: 0px;", vec![content]);
    // The theme scope's own body: no UA margin, the window's full height.
    ShellThemeScope::create(root)
        .with_accent(ShellThemeAccent::Clay)
        .body()
        .with_callback(
            EventFilter::Window(WindowEventFilter::CloseRequested),
            app.clone(),
            on_close_requested,
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            app,
            on_key,
        )
}

/// The source list's width was dragged: kept for the next rebuild.
extern "C" fn on_pane_resize(
    mut data: RefAny,
    _info: CallbackInfo,
    pane: usize,
    ratio: f32,
) -> Update {
    if let Some(mut s) = data.downcast_mut::<NewsApp>() {
        if pane == 0 {
            s.nav_ratio = ratio;
        }
    }
    Update::DoNothing
}

/// The splitter between the table and the reading pane was dragged: kept for the next rebuild.
extern "C" fn on_split_resize(
    mut data: RefAny,
    _info: CallbackInfo,
    state: SplitPaneState,
) -> Update {
    if let Some(mut s) = data.downcast_mut::<NewsApp>() {
        s.split_ratio = state.ratio;
    }
    Update::DoNothing
}

// ==== Callbacks: files and threads ====

/// Runs `f` on the app's state; the window is rebuilt afterwards.
fn with_app(
    app: &mut RefAny,
    info: &mut CallbackInfo,
    f: impl FnOnce(&mut NewsApp, &mut CallbackInfo, &RefAny),
) -> Update {
    let handle = app.clone();
    let Some(mut guard) = app.downcast_mut::<NewsApp>() else {
        return Update::DoNothing;
    };
    f(&mut guard, info, &handle);
    Update::RefreshDom
}

fn write_files(s: &NewsApp, info: &mut CallbackInfo, app: &RefAny, jobs: Vec<FileJob>, tag: u64) {
    kit::spawn_file_jobs(info, &s.data_root, jobs, app.clone(), tag, on_files_done);
}

/// Writes one feed's marks.
fn save_state(s: &NewsApp, info: &mut CallbackInfo, app: &RefAny, feed: usize) {
    if let Some(f) = s.library.feeds.get(feed) {
        write_files(s, info, app, vec![store::state_job(f)], TAG_WRITE);
    }
}

/// Writes the subscription list.
fn save_list(s: &mut NewsApp, info: &mut CallbackInfo, app: &RefAny) {
    write_list(s, info, app, TAG_WRITE);
}

/// Writes the subscription list as a job of `tag`.
fn write_list(s: &mut NewsApp, info: &mut CallbackInfo, app: &RefAny, tag: u64) {
    s.list_dirty = false;
    let job = store::subscriptions_job(&s.library);
    write_files(s, info, app, vec![job], tag);
}

/// (Re)starts the "Refresh every ..." timer for the setting: the running one stops, a new one
/// starts unless the setting is off.
fn arm_refresh_timer(s: &mut NewsApp, info: &mut CallbackInfo, app: &RefAny) {
    if let Some(id) = s.refresh_timer.take() {
        info.remove_timer(id);
    }
    if s.settings.refresh_minutes == 0 {
        return;
    }
    let every = u64::from(s.settings.refresh_minutes) * 60_000;
    let timer = Timer::create(app.clone(), on_refresh_timer, info.get_system_time_fn())
        .with_delay(Duration::System(SystemTimeDiff::from_millis(every)))
        .with_interval(Duration::System(SystemTimeDiff::from_millis(every)));
    let id = TimerId::unique();
    info.add_timer(id, timer);
    s.refresh_timer = Some(id);
}

/// The window is asked to close (its close button, the app's own): a feed page's renamed
/// title or folder is written when the page is left - so it is written now, and the window
/// closes once the write landed ([`on_files_done`], `TAG_CLOSING`).
extern "C" fn on_close_requested(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<NewsApp>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    if s.closing {
        // The list is on its way: the window closes when it landed.
        info.prevent_window_close();
        return Update::DoNothing;
    }
    if !s.list_dirty {
        return Update::DoNothing;
    }
    eprintln!("[aznews] the window closes once the subscription list is written");
    s.closing = true;
    info.prevent_window_close();
    write_list(s, &mut info, &app, TAG_CLOSING);
    Update::DoNothing
}

/// Refreshes the feeds at `feeds` on a Thread (the sample's documentation addresses are not
/// asked: nothing answers there with a feed).
fn start_refresh(s: &mut NewsApp, info: &mut CallbackInfo, app: &RefAny, feeds: Vec<usize>) {
    let jobs: Vec<RefreshJob> = feeds
        .into_iter()
        .filter_map(|i| s.library.feeds.get(i))
        .filter(|f| !links::is_documentation_host(&f.sub.url))
        .map(|f| RefreshJob {
            id: f.sub.id.clone(),
            url: f.sub.url.clone(),
            etag: f.meta.etag.clone(),
            last_modified: f.meta.last_modified.clone(),
        })
        .collect();
    if jobs.is_empty() {
        s.notice = "Nothing to refresh.".to_string();
        return;
    }
    s.notice.clear();
    if s.refreshing == 0 {
        s.refresh_total = 0;
    }
    s.refreshing += jobs.len();
    s.refresh_total += jobs.len();
    jobs::spawn_refresh(info, jobs, app.clone(), on_refresh_event);
}

/// The feeds a "Get News" asks: every followed one.
fn followed_feeds(s: &NewsApp) -> Vec<usize> {
    s.library.followed()
}

/// Fetches the open article's pictures that are not in the cache yet (when they are allowed).
fn ask_pictures(s: &mut NewsApp, info: &mut CallbackInfo, app: &RefAny) {
    let Some(r) = s.selected_ref() else {
        return;
    };
    let article = {
        let item = &s.library.feeds[r.feed].items[r.item];
        if !s.pictures_on(item) {
            return;
        }
        s.article_view(item)
    };
    let urls: Vec<String> = article
        .images
        .into_iter()
        .filter(|u| {
            !s.pictures.contains(u)
                && !s.pictures_asked.contains(u)
                && !s.pictures_failed.contains(u)
        })
        .collect();
    for u in &urls {
        s.pictures_asked.insert(u.clone());
    }
    jobs::spawn_pictures(info, urls, app.clone(), on_picture_event);
}

/// Opens an article: selected, marked read (its marks written), its pictures asked for.
fn select(s: &mut NewsApp, info: &mut CallbackInfo, app: &RefAny, r: ArticleRef) {
    let Some(reference) = s.reference_of(r) else {
        return;
    };
    println!("AZNEWS_SELECTED {} {}", reference.0, reference.1);
    s.selected = Some(reference);
    s.reading = Reading::Article;
    s.confirm_unsubscribe = false;
    if s.library.set_read(r, true) {
        save_state(s, info, app, r.feed);
    }
    ask_pictures(s, info, app);
}

/// Leaves a form for the article (a feed page's changes to the list are written).
fn leave_form(s: &mut NewsApp, info: &mut CallbackInfo, app: &RefAny) {
    if s.list_dirty {
        save_list(s, info, app);
    }
    s.reading = Reading::Article;
    s.confirm_unsubscribe = false;
    s.confirm_remove = None;
}

/// Marks the articles `refs` read and writes the marks of the feeds that changed; how many
/// were unread.
fn mark_read(s: &mut NewsApp, info: &mut CallbackInfo, app: &RefAny, refs: &[ArticleRef]) -> usize {
    let unread = refs.iter().filter(|r| !s.library.is_read(**r)).count();
    let changed = s.library.mark_all_read(refs);
    let jobs: Vec<FileJob> = changed
        .iter()
        .filter_map(|&f| s.library.feeds.get(f).map(store::state_job))
        .collect();
    if !jobs.is_empty() {
        write_files(s, info, app, jobs, TAG_WRITE);
    }
    unread
}

/// Unsubscribes from the feed `id`: its files deleted, the list written, the view reset.
fn unsubscribe(s: &mut NewsApp, info: &mut CallbackInfo, app: &RefAny, id: &str) {
    if s.library.unsubscribe(id).is_some() {
        println!("AZNEWS_UNSUBSCRIBED {id}");
        write_files(s, info, app, store::delete_jobs(id), TAG_WRITE);
        save_list(s, info, app);
    }
    if s.selected.as_ref().is_some_and(|(feed, _)| feed == id) {
        s.selected = None;
    }
    s.source_open.remove(id);
    s.confirm_unsubscribe = false;
    s.confirm_remove = None;
    let shown = match &s.view {
        View::Feed(feed) | View::Topic(feed, _) => feed == id,
        _ => false,
    };
    if shown {
        set_view(s, View::All);
    }
}

/// Follows the feed at `index` again, or stops following it: the list written; a feed followed
/// again is refreshed.
fn set_followed(s: &mut NewsApp, info: &mut CallbackInfo, app: &RefAny, index: usize, on: bool) {
    if !s.library.set_paused(index, !on) {
        return;
    }
    let id = s.library.feeds[index].sub.id.clone();
    println!("AZNEWS_FOLLOW {id} {}", if on { "on" } else { "off" });
    s.notice = format!(
        "{} {}",
        s.library.feeds[index].name(),
        if on {
            "is followed again"
        } else {
            "is no longer followed"
        }
    );
    save_list(s, info, app);
    if on {
        start_refresh(s, info, app, vec![index]);
    }
}

extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<NewsApp>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    kit::on_window_created(&s.kit, &mut info);
    kit::spawn_file_jobs(
        &mut info,
        &s.data_root,
        store::load_jobs(),
        app.clone(),
        TAG_LOAD,
        on_files_done,
    );
    arm_refresh_timer(s, &mut info, &app);
    Update::DoNothing
}

/// "Refresh every ...": every followed feed, unless a refresh is running.
extern "C" fn on_refresh_timer(
    mut data: RefAny,
    mut info: TimerCallbackInfo,
) -> TimerCallbackReturn {
    let app = data.clone();
    let Some(mut s) = data.downcast_mut::<NewsApp>() else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    if !s.loaded || s.refreshing > 0 || s.settings.refresh_minutes == 0 {
        return TimerCallbackReturn::continue_unchanged();
    }
    let feeds = followed_feeds(&s);
    start_refresh(&mut s, &mut info.callback_info, &app, feeds);
    TimerCallbackReturn::continue_and_refresh_dom()
}

/// What the load jobs read: the library, the sample on an empty folder, the start screen.
fn after_load(s: &mut NewsApp, info: &mut CallbackInfo, app: &RefAny, outcomes: Vec<FileOutcome>) {
    let mut subscriptions: Option<Vec<u8>> = None;
    let mut files = Vec::new();
    for outcome in outcomes {
        match outcome {
            FileOutcome::Got {
                result: Ok(bytes), ..
            } => subscriptions = bytes,
            FileOutcome::Got {
                result: Err(e),
                key,
            } => eprintln!("[aznews] {key}: {e}"),
            FileOutcome::GotAll {
                files: found,
                errors,
                ..
            } => {
                files = found;
                for e in errors {
                    eprintln!("[aznews] {e}");
                }
            }
            _ => {}
        }
    }
    let mut mint = azul_storage::ids::new_uuid;
    let loaded = store::load(subscriptions.as_deref(), &files, &mut mint);
    for p in &loaded.problems {
        eprintln!("[aznews] {p}");
    }
    if !loaded.problems.is_empty() {
        s.notice = format!(
            "{} file(s) could not be read fully - see the log",
            loaded.problems.len()
        );
    }
    s.library = loaded.library;
    let mut sample_made = false;
    if s.library.feeds.is_empty() && s.sample {
        let now = now_secs();
        s.library = sample::sample_library(now);
        let mut jobs = vec![store::subscriptions_job(&s.library)];
        for f in &s.library.feeds {
            jobs.extend(store::feed_jobs(f));
        }
        write_files(s, info, app, jobs, TAG_SAMPLE);
        s.last_refresh = now - 600;
        sample_made = true;
    } else if loaded.minted {
        save_list(s, info, app);
    }
    s.loaded = true;
    let articles: usize = s.library.feeds.iter().map(|f| f.items.len()).sum();
    println!("AZNEWS_LOADED {} {articles}", s.library.feeds.len());
    match std::mem::take(&mut s.start_screen).as_str() {
        "add" => s.reading = Reading::AddFeed(AddFeed::default()),
        "import" => s.reading = Reading::Import(OpmlImport::default()),
        "sources" => s.reading = Reading::Sources,
        "feed" => {
            if let Some(f) = s.library.feeds.first() {
                s.reading = Reading::Feed(f.sub.id.clone());
            }
        }
        _ => {
            // The table's first article opens (and so is read), as in Mail's message view.
            if let Some(first) = s.ordered().first().copied() {
                select(s, info, app, first);
            }
        }
    }
    if let Some(path) = s.import_files.first().cloned() {
        s.import_files.clear();
        s.reading = Reading::Import(OpmlImport::default());
        read_import_file(s, info, app, &path);
    }
    if s.settings.refresh_on_start && !sample_made {
        let feeds = followed_feeds(s);
        start_refresh(s, info, app, feeds);
    }
}

/// The OPML file the import read.
fn import_file_read(s: &mut NewsApp, outcomes: Vec<FileOutcome>) {
    let Reading::Import(st) = &mut s.reading else {
        return;
    };
    st.reading = false;
    for outcome in outcomes {
        match outcome {
            FileOutcome::Got {
                result: Ok(Some(bytes)),
                ..
            } => match opml::parse(&bytes) {
                Ok(subs) => {
                    st.problem = if subs.is_empty() {
                        "The file lists no feed.".to_string()
                    } else {
                        String::new()
                    };
                    st.rows = import_rows(subs, &s.library);
                    println!("AZNEWS_IMPORT_PREVIEW {}", st.rows.len());
                }
                Err(e) => st.problem = e,
            },
            FileOutcome::Got {
                result: Ok(None),
                key,
            } => st.problem = format!("\u{201c}{key}\u{201d} does not exist."),
            FileOutcome::Got { result: Err(e), .. } => st.problem = e,
            _ => {}
        }
    }
}

extern "C" fn on_files_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, info, handle| match reply.tag {
        TAG_LOAD => after_load(s, info, handle, reply.outcomes),
        TAG_IMPORT_FILE => import_file_read(s, reply.outcomes),
        _ => {
            let mut failed = 0;
            for outcome in &reply.outcomes {
                if let Some(e) = outcome.error() {
                    failed += 1;
                    eprintln!("[aznews] {e}");
                    continue;
                }
                if let FileOutcome::Put { key, .. } = outcome {
                    println!("AZNEWS_SAVED {key}");
                    if key.starts_with(EXPORTS_PREFIX) {
                        println!("AZNEWS_EXPORTED {key}");
                        s.notice = format!(
                            "Exported to {}",
                            azul_appkit::data::local_path(&s.data_root, key).display()
                        );
                    }
                }
            }
            if reply.tag == TAG_SAMPLE {
                println!("AZNEWS_SAMPLE_WRITTEN {}", reply.outcomes.len() - failed);
            }
            if failed > 0 {
                s.notice = format!("{failed} file(s) could not be written - see the log");
            }
            if reply.tag == TAG_CLOSING && s.closing {
                s.closing = false;
                if failed == 0 {
                    eprintln!("[aznews] the subscription list landed: the window closes");
                    info.close_window();
                } else {
                    // Not lost without a word: the window stays and says so; the next close
                    // quits without the list.
                    s.notice = "The subscription list could not be written - see the log. \
                                Close the window again to quit without it."
                        .to_string();
                }
            }
        }
    })
}

/// Where exports go (in the data tree, house rule).
const EXPORTS_PREFIX: &str = "news/exports/";

extern "C" fn on_refresh_event(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(event) = jobs::take::<RefreshEvent>(&mut msg) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, info, handle| match event {
        RefreshEvent::Done => {
            s.last_refresh = now_secs();
            println!("AZNEWS_REFRESH_DONE {}", s.library.unread_total());
        }
        RefreshEvent::Fetched { id, fetched, now } => {
            s.refreshing = s.refreshing.saturating_sub(1);
            let Some(i) = s.library.feed_index(&id) else {
                return;
            };
            let keep = s.settings.keep_days;
            let outcome = match fetched {
                Fetched::Feed {
                    feed,
                    etag,
                    last_modified,
                    status,
                } => {
                    let fresh = s.library.merge(i, feed, now, keep);
                    let meta = &mut s.library.feeds[i].meta;
                    meta.etag = etag;
                    meta.last_modified = last_modified;
                    meta.status = status;
                    fresh.to_string()
                }
                Fetched::NotModified { status } => {
                    let meta = &mut s.library.feeds[i].meta;
                    meta.checked = now;
                    meta.status = status;
                    meta.error.clear();
                    "304".to_string()
                }
                Fetched::Page { .. } => {
                    let meta = &mut s.library.feeds[i].meta;
                    meta.checked = now;
                    meta.error = "this address is a web page, not a feed".to_string();
                    "error".to_string()
                }
                Fetched::Failed { status, error } => {
                    let meta = &mut s.library.feeds[i].meta;
                    meta.checked = now;
                    meta.status = status;
                    meta.error = error;
                    "error".to_string()
                }
            };
            println!("AZNEWS_REFRESHED {id} {outcome}");
            let jobs = store::feed_jobs(&s.library.feeds[i]);
            write_files(s, info, handle, jobs, TAG_WRITE);
        }
    })
}

extern "C" fn on_picture_event(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(event) = jobs::take::<PictureEvent>(&mut msg) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, info, _| {
        let PictureEvent { url, image, error } = event;
        s.pictures_asked.remove(&url);
        match image.and_then(|raw| ImageRef::create_rawimage(raw).into_option()) {
            Some(image) => {
                info.add_image_to_cache(url.as_str(), image);
                println!("AZNEWS_PICTURE {url}");
                s.pictures.insert(url);
            }
            None => {
                eprintln!("[aznews] picture {url} not shown: {error}");
                s.pictures_failed.insert(url);
            }
        }
    })
}

extern "C" fn on_find_event(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(event) = jobs::take::<FindEvent>(&mut msg) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, _info, _| {
        let Reading::AddFeed(st) = &mut s.reading else {
            return;
        };
        if st.input.trim() != event.input.trim() {
            // An answer for what was typed before: a newer Find is on its way.
            return;
        }
        st.finding = false;
        match event.result {
            Ok(candidates) => {
                println!("AZNEWS_FOUND {}", candidates.len());
                st.problem.clear();
                st.chosen = 0;
                st.candidates = candidates;
            }
            Err(problem) => {
                println!("AZNEWS_FOUND 0");
                st.problem = problem;
                st.candidates.clear();
            }
        }
    })
}

// ==== Callbacks: the view, the search, mark all ====

/// Shows `view` in the table (its source's section opens).
fn set_view(s: &mut NewsApp, view: View) {
    if let View::Feed(id) | View::Topic(id, _) = &view {
        s.source_open.insert(id.clone(), true);
    }
    s.view = view;
    s.confirm_mark_all = false;
    println!("AZNEWS_VIEW {}", s.list().len());
}

extern "C" fn on_search(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let query = state.get_text().as_str().to_string();
    let update = with_app(&mut data, &mut info, |s, info, handle| {
        if !matches!(s.reading, Reading::Article) {
            leave_form(s, info, handle);
        }
        s.query = query;
        println!("AZNEWS_VIEW {}", s.list().len());
    });
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_filter(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        s.unread_only = state.selected_index == 1;
        println!("AZNEWS_VIEW {}", s.list().len());
    })
}

/// "Mark all as read": asked first, over the table.
extern "C" fn on_mark_all(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        leave_form(s, info, handle);
        s.confirm_mark_all = true;
    })
}

extern "C" fn on_mark_all_no(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        s.confirm_mark_all = false
    })
}

extern "C" fn on_mark_all_yes(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        s.confirm_mark_all = false;
        let list = s.list();
        let unread = mark_read(s, info, handle, &list);
        println!("AZNEWS_MARKED_ALL {unread}");
        s.notice = format!("{unread} article(s) marked as read");
    })
}

// ==== Callbacks: the article ====

/// The open article's place in the table and the next / previous one (scrolled into view).
fn step(s: &mut NewsApp, info: &mut CallbackInfo, app: &RefAny, forward: bool) {
    let list = s.ordered();
    if let Some(r) = s.library.next(&list, s.selected_ref(), forward) {
        select(s, info, app, r);
        table::reveal_selected(s, info);
    }
}

extern "C" fn on_prev(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        step(s, info, handle, false)
    })
}

extern "C" fn on_next(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        step(s, info, handle, true)
    })
}

extern "C" fn on_star(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        if let Some(r) = s.selected_ref() {
            s.library.toggle_star(r);
            save_state(s, info, handle, r.feed);
        }
    })
}

extern "C" fn on_later(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        if let Some(r) = s.selected_ref() {
            s.library.toggle_later(r);
            save_state(s, info, handle, r.feed);
        }
    })
}

extern "C" fn on_toggle_read(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        if let Some(r) = s.selected_ref() {
            let read = s.library.is_read(r);
            s.library.set_read(r, !read);
            save_state(s, info, handle, r.feed);
        }
    })
}

extern "C" fn on_open_original(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        let Some(link) = s.selected_link() else {
            return;
        };
        if let Err(e) = azul_appkit::files::open_external(&link) {
            s.notice = e;
        }
    })
}

/// "Copy Link": the open article's address on the clipboard (to share it anywhere).
extern "C" fn on_copy_link(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        let Some(link) = s.selected_link() else {
            s.notice = "This article has no link.".to_string();
            return;
        };
        info.set_clipboard_content(ClipboardContent {
            plain_text: AzString::from(link.as_str()),
            styled_runs: StyledTextRunVec::create(),
            html: OptionString::None,
        });
        println!("AZNEWS_COPIED {link}");
        s.notice = "The article's link is on the clipboard.".to_string();
    })
}

extern "C" fn on_reading_event(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: ReadingPaneEvent,
) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| match event.kind {
        ReadingPaneEventKind::LoadImages => {
            if let Some(id) = s.selected.as_ref().map(|(_, item)| item.clone()) {
                s.pictures_allowed.insert(id);
                ask_pictures(s, info, handle);
            }
        }
        // The feed's name: its page.
        ReadingPaneEventKind::Sender => {
            if let Some((feed, _)) = s.selected.clone() {
                s.reading = Reading::Feed(feed);
            }
        }
        ReadingPaneEventKind::People | ReadingPaneEventKind::Attachment => {}
    })
}

// ==== Callbacks: Add feed ====

extern "C" fn on_add_open(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        leave_form(s, info, handle);
        let folder = match &s.view {
            View::Folder(f) => f.clone(),
            View::Feed(id) | View::Topic(id, _) => s
                .library
                .feed_index(id)
                .map_or_else(String::new, |i| s.library.feeds[i].sub.folder.clone()),
            _ => String::new(),
        };
        s.reading = Reading::AddFeed(AddFeed {
            folder,
            ..AddFeed::default()
        });
    })
}

extern "C" fn on_leave(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        leave_form(s, info, handle)
    })
}

extern "C" fn on_add_input(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let text = state.get_text().as_str().to_string();
    let update = with_app(&mut data, &mut info, |s, _info, _| {
        if let Reading::AddFeed(st) = &mut s.reading {
            st.input = text;
        }
    });
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_add_folder(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let text = state.get_text().as_str().to_string();
    let update = with_app(&mut data, &mut info, |s, _info, _| {
        if let Reading::AddFeed(st) = &mut s.reading {
            st.folder = text;
        }
    });
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_add_find(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        let Reading::AddFeed(st) = &mut s.reading else {
            return;
        };
        if st.input.trim().is_empty() {
            st.problem = "Type the address of a website or of a feed.".to_string();
            return;
        }
        st.finding = true;
        st.problem.clear();
        st.candidates.clear();
        let input = st.input.trim().to_string();
        jobs::spawn_find(info, input, handle.clone(), on_find_event);
    })
}

extern "C" fn on_add_pick(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data
        .downcast_ref::<PickRef>()
        .map(|p| (p.app.clone(), p.index))
    else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, _info, _| {
        if let Reading::AddFeed(st) = &mut s.reading {
            st.chosen = index;
        }
    })
}

extern "C" fn on_add_subscribe(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        let (candidate, folder) = match &s.reading {
            Reading::AddFeed(st) => match st.candidates.get(st.chosen) {
                Some(c) => (c.clone(), st.folder.trim().to_string()),
                None => return,
            },
            _ => return,
        };
        let Candidate {
            url,
            link_title,
            feed,
            etag,
            last_modified,
        } = candidate;
        let id = azul_storage::ids::new_uuid();
        let title = if !feed.title.is_empty() {
            feed.title.clone()
        } else if !link_title.is_empty() {
            link_title
        } else {
            url.clone()
        };
        let index = s.library.subscribe(Subscription {
            id: id.clone(),
            title,
            url: url.clone(),
            site: feed.site.clone(),
            folder,
            paused: false,
        });
        let feed_id = s.library.feeds[index].sub.id.clone();
        if feed_id == id {
            let keep = s.settings.keep_days;
            s.library.merge(index, feed, now_secs(), keep);
            let meta = &mut s.library.feeds[index].meta;
            meta.etag = etag;
            meta.last_modified = last_modified;
            meta.status = 200;
        }
        println!("AZNEWS_SUBSCRIBED {feed_id} {url}");
        let mut jobs = vec![store::subscriptions_job(&s.library)];
        jobs.extend(store::feed_jobs(&s.library.feeds[index]));
        write_files(s, info, handle, jobs, TAG_WRITE);
        s.reading = Reading::Article;
        set_view(s, View::Feed(feed_id));
    })
}

// ==== Callbacks: OPML ====

extern "C" fn on_import_open(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        leave_form(s, info, handle);
        s.reading = Reading::Import(OpmlImport::default());
    })
}

extern "C" fn on_import_path(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let text = state.get_text().as_str().to_string();
    let update = with_app(&mut data, &mut info, |s, _info, _| {
        if let Reading::Import(st) = &mut s.reading {
            st.path = text;
        }
    });
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// Reads an OPML file the user named (outside the data tree) on a Thread.
fn read_import_file(s: &mut NewsApp, info: &mut CallbackInfo, app: &RefAny, path: &Path) {
    let Reading::Import(st) = &mut s.reading else {
        return;
    };
    st.path = path.display().to_string();
    st.rows.clear();
    st.problem.clear();
    if kit::spawn_outside_read(info, path, app.clone(), TAG_IMPORT_FILE, on_files_done) {
        st.reading = true;
    } else {
        st.problem = format!("\u{201c}{}\u{201d} is not a file.", path.display());
    }
}

extern "C" fn on_import_read(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        let path = match &s.reading {
            Reading::Import(st) => st.path.trim().to_string(),
            _ => return,
        };
        if path.is_empty() {
            if let Reading::Import(st) = &mut s.reading {
                st.problem = "Type the path of an .opml file, or choose one.".to_string();
            }
            return;
        }
        read_import_file(s, info, handle, &PathBuf::from(path));
    })
}

extern "C" fn on_import_choose(mut data: RefAny, _info: CallbackInfo) -> Update {
    let app = data.clone();
    if data.downcast_ref::<NewsApp>().is_none() {
        return Update::DoNothing;
    }
    let _request = FileDialog::open_file(
        "Import subscriptions",
        OptionString::None,
        OptionFileTypeList::None,
        app,
        on_import_file_picked,
    );
    Update::DoNothing
}

extern "C" fn on_import_file_picked(
    mut data: RefAny,
    mut info: CallbackInfo,
    result: RefAny,
) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing;
    };
    let path = PathBuf::from(path.as_string().as_str());
    with_app(&mut data, &mut info, |s, info, handle| {
        read_import_file(s, info, handle, &path)
    })
}

extern "C" fn on_import_toggle(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: CheckBoxState,
) -> Update {
    let Some((mut app, index)) = data
        .downcast_ref::<ImportRowRef>()
        .map(|r| (r.app.clone(), r.index))
    else {
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

extern "C" fn on_import_run(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        let rows = match &s.reading {
            Reading::Import(st) => st.rows.clone(),
            _ => return,
        };
        let before = s.library.feeds.len();
        for row in rows.into_iter().filter(|r| r.selected && !r.known) {
            let mut sub = row.sub;
            sub.id = azul_storage::ids::new_uuid();
            s.library.subscribe(sub);
        }
        let added: Vec<usize> = (before..s.library.feeds.len()).collect();
        println!("AZNEWS_IMPORTED {}", added.len());
        s.notice = format!("{} feed(s) imported", added.len());
        save_list(s, info, handle);
        s.reading = Reading::Article;
        let followed: Vec<usize> = added
            .into_iter()
            .filter(|&i| !s.library.feeds[i].sub.paused)
            .collect();
        start_refresh(s, info, handle, followed);
    })
}

extern "C" fn on_export(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        let key = format!("{EXPORTS_PREFIX}subscriptions-{}.opml", now_secs());
        let bytes = opml::write(&s.library.subscriptions(), store::OPML_TITLE).into_bytes();
        write_files(
            s,
            info,
            handle,
            vec![FileJob::Put { key, bytes }],
            TAG_WRITE,
        );
    })
}

// ==== Callbacks: a feed's page, the sources page ====

/// The feed of the page that is open.
fn page_feed(s: &NewsApp) -> Option<usize> {
    match &s.reading {
        Reading::Feed(id) => s.library.feed_index(id),
        _ => None,
    }
}

extern "C" fn on_feed_title(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let text = state.get_text().as_str().to_string();
    let update = with_app(&mut data, &mut info, |s, _info, _| {
        if let Some(i) = page_feed(s) {
            s.library.feeds[i].sub.title = text;
            s.list_dirty = true;
        }
    });
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_feed_folder(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let text = state.get_text().as_str().trim().to_string();
    let update = with_app(&mut data, &mut info, |s, _info, _| {
        if let Some(i) = page_feed(s) {
            s.library.feeds[i].sub.folder = text;
            s.list_dirty = true;
        }
    });
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_feed_refresh(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        if let Some(i) = page_feed(s) {
            start_refresh(s, info, handle, vec![i]);
        }
    })
}

extern "C" fn on_feed_articles(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        let Some(id) = page_feed(s).map(|i| s.library.feeds[i].sub.id.clone()) else {
            return;
        };
        leave_form(s, info, handle);
        set_view(s, View::Feed(id));
    })
}

extern "C" fn on_unsubscribe(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        s.confirm_unsubscribe = true
    })
}

extern "C" fn on_unsubscribe_confirmed(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        let Some(id) = page_feed(s).map(|i| s.library.feeds[i].sub.id.clone()) else {
            return;
        };
        unsubscribe(s, info, handle, &id);
        s.reading = Reading::Article;
        set_view(s, View::All);
    })
}

/// The sources page (the toolbar's "Sources", the actions menu).
extern "C" fn on_sources_open(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        leave_form(s, info, handle);
        s.reading = Reading::Sources;
    })
}

// ==== Callbacks: toolbar, settings, keys ====

/// "Get News": every followed feed.
extern "C" fn on_refresh(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        if s.refreshing == 0 {
            let feeds = followed_feeds(s);
            start_refresh(s, info, handle, feeds);
        }
    })
}

/// Cancel on the settings page put the settings back: the app's copy follows (and the refresh
/// timer, when its interval changed back).
fn reload_settings(app: &mut RefAny, info: &mut CallbackInfo, settings: &AppSettings) {
    let read = Settings::read(&|key| settings.get(key).map(str::to_string));
    let _update = with_app(app, info, |s, info, handle| {
        let rearm = s.settings.refresh_minutes != read.refresh_minutes;
        s.settings = read;
        if rearm {
            arm_refresh_timer(s, info, handle);
        }
    });
}

extern "C" fn on_open_settings(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        kit::open_settings(&s.kit, None)
    })
}

extern "C" fn on_set_font(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: SegmentedState,
) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        if let Some(px) = FONT_SIZES.get(state.selected_index) {
            s.settings.font_px = *px;
            kit::set_value(&s.kit, info, "font", &px.to_string());
        }
    })
}

extern "C" fn on_set_measure(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: SegmentedState,
) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        if let Some((_, px)) = MEASURES.get(state.selected_index) {
            s.settings.measure_px = *px;
            kit::set_value(&s.kit, info, "measure", &px.to_string());
        }
    })
}

extern "C" fn on_set_paper(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: SegmentedState,
) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        s.settings.sepia = state.selected_index == 1;
        kit::set_value(
            &s.kit,
            info,
            "paper",
            if s.settings.sepia { "sepia" } else { "theme" },
        );
    })
}

extern "C" fn on_set_pictures(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: SegmentedState,
) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        if let Some(p) = Pictures::ALL.get(state.selected_index) {
            s.settings.pictures = *p;
            kit::set_value(&s.kit, info, "pictures", p.key());
        }
    })
}

fn flag(on: bool) -> &'static str {
    if on {
        "true"
    } else {
        "false"
    }
}

extern "C" fn on_set_strip(mut data: RefAny, mut info: CallbackInfo, state: SwitchState) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        s.settings.strip_tracking = state.checked;
        kit::set_value(&s.kit, info, "strip", flag(state.checked));
    })
}

extern "C" fn on_set_refresh_start(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: SwitchState,
) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        s.settings.refresh_on_start = state.checked;
        kit::set_value(&s.kit, info, "refresh_start", flag(state.checked));
    })
}

extern "C" fn on_set_refresh_every(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: SegmentedState,
) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        if let Some((_, minutes)) = REFRESH_EVERY.get(state.selected_index) {
            s.settings.refresh_minutes = *minutes;
            kit::set_value(&s.kit, info, "refresh_every", &minutes.to_string());
            // The new interval takes effect now, not at the next start.
            arm_refresh_timer(s, info, handle);
        }
    })
}

extern "C" fn on_set_keep(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: SegmentedState,
) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        if let Some((_, days)) = KEEP.get(state.selected_index) {
            s.settings.keep_days = *days;
            kit::set_value(&s.kit, info, "keep", &days.to_string());
        }
    })
}

extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(kit_ref) = data.downcast_ref::<NewsApp>().map(|s| s.kit.clone()) else {
        return Update::DoNothing;
    };
    if let Some(update) = kit::handle_key(&kit_ref, &mut info) {
        return update;
    }
    if kit::settings_open(&kit_ref) {
        return Update::DoNothing;
    }
    let Some(key) = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option()
    else {
        return Update::DoNothing;
    };
    let m = info.get_key_modifiers();
    let command = m.primary_down();
    use azul::dom::VirtualKeyCode as K;
    if command {
        return match key {
            K::N => {
                info.prevent_default();
                on_add_open(data, info)
            }
            K::O => {
                info.prevent_default();
                on_import_open(data, info)
            }
            K::E => {
                info.prevent_default();
                on_export(data, info)
            }
            _ => Update::DoNothing,
        };
    }
    if key == K::Escape {
        return with_app(&mut data, &mut info, |s, info, handle| {
            s.confirm_mark_all = false;
            leave_form(s, info, handle);
        });
    }
    // Plain keys are the table's only while no field has the focus (typing "j" into the search
    // box types a "j").
    if info.get_focused_node().into_option().is_some() {
        return Update::DoNothing;
    }
    match key {
        K::J | K::Down => on_next(data, info),
        K::K | K::Up => on_prev(data, info),
        K::S => on_star(data, info),
        K::M => on_toggle_read(data, info),
        K::L => on_later(data, info),
        K::O => on_open_original(data, info),
        K::R => on_refresh(data, info),
        _ => Update::DoNothing,
    }
}

#[cfg(test)]
mod tests {
    //! The parts of the window that are plain data.

    use super::*;

    #[test]
    fn ages_read_like_the_list_says_them() {
        let now = 1_790_856_000;
        assert_eq!(age(now - 10, now), "now");
        assert_eq!(age(now - 5 * 60, now), "5 min");
        assert_eq!(age(now - 2 * 3_600 - 5, now), "2 h");
        assert_eq!(age(now - 3 * DAY, now), "3 d");
        assert_eq!(age(now + 600, now), "now", "a date in the future is now");
        assert_eq!(
            age(now - 40 * DAY, now).len(),
            10,
            "a date: {}",
            age(now - 40 * DAY, now)
        );
    }

    #[test]
    fn the_activity_says_when_the_feeds_were_last_asked() {
        let now = 1_790_856_000;
        assert_eq!(updated_line(0, now), "Not refreshed yet");
        assert_eq!(updated_line(now - 5, now), "Updated just now");
        assert_eq!(updated_line(now - 5 * 60, now), "Updated 5 min ago");
        assert!(
            !updated_line(now - 40 * DAY, now).ends_with(" ago"),
            "a date is no age: {}",
            updated_line(now - 40 * DAY, now)
        );
    }

    #[test]
    fn settings_are_read_back_inside_what_is_offered() {
        let values = |key: &str| -> Option<String> {
            match key {
                "font" => Some("22".into()),
                "measure" => Some("999".into()),
                "paper" => Some("sepia".into()),
                "pictures" => Some("always".into()),
                "strip" => Some("false".into()),
                "refresh_every" => Some("15".into()),
                "keep" => Some("banana".into()),
                "sort" => Some("title-asc".into()),
                _ => None,
            }
        };
        let s = Settings::read(&values);
        assert_eq!(s.font_px, 22);
        assert_eq!(s.measure_px, 680, "not offered: the default");
        assert!(s.sepia);
        assert_eq!(s.pictures, Pictures::Always);
        assert!(!s.strip_tracking);
        assert!(s.refresh_on_start, "unset: the default");
        assert_eq!(s.refresh_minutes, 15);
        assert_eq!(s.keep_days, 30);
        assert_eq!(
            s.sort,
            Sort {
                key: SortKey::Title,
                descending: false,
            }
        );
        assert_eq!(
            Settings::read(&|_: &str| -> Option<String> { None }),
            Settings::default()
        );
    }

    #[test]
    fn the_table_order_round_trips_through_its_setting() {
        for key in SortKey::ALL {
            for descending in [false, true] {
                let sort = Sort { key, descending };
                assert_eq!(parse_sort(Some(sort_value(sort).as_str())), sort);
            }
        }
        assert_eq!(sort_value(Sort::default()), "date-desc");
        assert_eq!(parse_sort(None), Sort::default());
        assert_eq!(parse_sort(Some("nonsense")), Sort::default());
        assert_eq!(
            parse_sort(Some("unread-sideways")),
            Sort {
                key: SortKey::Unread,
                descending: true,
            },
            "an unknown direction: the column's own"
        );
    }

    #[test]
    fn an_import_preview_marks_what_is_subscribed_already() {
        let lib = sample::sample_library(1_790_856_000);
        let known = lib.feeds[0].sub.clone();
        let rows = import_rows(
            vec![
                known.clone(),
                Subscription {
                    id: "x".into(),
                    title: "New".into(),
                    url: "https://new.example.org/feed".into(),
                    ..Subscription::default()
                },
            ],
            &lib,
        );
        assert!(rows[0].known && !rows[0].selected);
        assert!(!rows[1].known && rows[1].selected);
        assert_eq!(rows[1].sub.id, "", "an id from another file is not ours");
    }
}
