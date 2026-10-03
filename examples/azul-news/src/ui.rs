//! AzNews' window on azul's S4 `PimShell` (Outlook 2010's three panes): the app-drawn title row
//! (`NoTitle` + `Titlebar`), a toolbar in the ribbon row (Refresh, Add feed, Import / Export
//! OPML, Mark all as read, Settings), the navigation pane (`ShellNavigationPane`: all articles,
//! unread, starred, read later, broken feeds; the folders with their feeds and unread counts),
//! the article list (search, all / unread, Outlook's date groups, unread dots, two lines of
//! text) and the reading pane: azul's `ReadingPane` with the article read through azul's
//! HTML5-like parser in the reader stylesheet ([`crate::reader::article`]), its pictures fetched
//! on a Thread and put into the image cache ([`crate::jobs::spawn_pictures`]); or the Add-feed
//! form, the OPML import preview, a feed's page. A status bar with the last refresh and the
//! unread count.
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
//! `AZNEWS_UNSUBSCRIBED <feed id>`, `AZNEWS_MARKED_ALL <n>`.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, ReadingPaneOnEventCallbackType,
        SegmentedOnChangeCallbackType, ShellNavigationPaneOnEventCallbackType, SwitchOnToggleCallbackType,
        TextInputOnTextInputCallbackType, TimerCallbackInfo, TimerCallbackReturn,
    },
    dialog::{FileDialog, FileOpenResult},
    image::ImageRef,
    option::{OptionFileTypeList, OptionString},
    prelude::*,
    shells::{
        PimShell, ShellEmptyState, ShellNavigationGroup, ShellNavigationPane, ShellNavigationPaneEvent,
        ShellNavigationPaneEventKind, ShellThemeAccent, ShellThemeScope,
    },
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    vec::StringVec,
    widgets::{
        ButtonType, CheckBox, CheckBoxState, InfoBar, OnTextInputReturn, ReadingPane, ReadingPaneEvent,
        ReadingPaneEventKind, Segmented, SegmentedState, StatusBar, StatusBarSegment, Switch, SwitchState,
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
use azul_pim::dates::DateGroup;

use crate::{
    feed::Item,
    fetch::{Candidate, Fetched},
    ids,
    jobs::{self, FindEvent, PictureEvent, RefreshEvent, RefreshJob},
    library::{ArticleRef, Library, View, DAY},
    opml::{self, Subscription},
    reader, sample, store,
};

// ==== The app's facts ====

pub const SCREENS: [&str; 5] = ["articles", "add", "import", "feed", "settings"];

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
    Shortcut::new("Feeds", "R", "Refresh every feed"),
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

/// Articles the list shows before "Show more".
const LIST_PAGE: usize = 300;

/// The reading font sizes offered (px).
pub const FONT_SIZES: [u32; 5] = [16, 18, 20, 22, 24];
/// The line widths offered: (label, px).
pub const MEASURES: [(&str, u32); 3] = [("Narrow", 560), ("Medium", 680), ("Wide", 820)];
/// How often feeds are refreshed while the window is open: (label, minutes; 0 = never).
pub const REFRESH_EVERY: [(&str, u32); 4] = [("Never", 0), ("15 min", 15), ("Hourly", 60), ("4 hours", 240)];
/// How long an article the feed dropped is kept: (label, days).
pub const KEEP: [(&str, u32); 3] = [("A week", 7), ("A month", 30), ("Three months", 90)];

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
        Pictures::ALL.into_iter().find(|p| Some(p.key()) == key).unwrap_or(Pictures::OnClick)
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
            .map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string())
            .unwrap_or_default()
    }
}

/// An article's date for the reading pane: `Wednesday, 30 September 2026, 10:42` (local time).
#[must_use]
pub fn long_date(date: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(date, 0)
        .map(|d| d.with_timezone(&chrono::Local).format("%A, %-d %B %Y, %H:%M").to_string())
        .unwrap_or_default()
}

/// The local offset from UTC now, in seconds (the list's date groups).
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

/// What the reading pane shows.
#[derive(Debug, Clone)]
pub enum Reading {
    /// The selected article (or the empty state).
    Article,
    AddFeed(AddFeed),
    Import(OpmlImport),
    /// A feed's page, by its id.
    Feed(String),
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
    pub nav_open: [bool; 2],
    pub notice: String,
    /// Feeds still to answer in the running refresh.
    pub refreshing: usize,
    /// When the last refresh ended.
    pub last_refresh: i64,
    /// Pictures in azul's image cache, asked for, and failed (by address).
    pub pictures: BTreeSet<String>,
    pub pictures_asked: BTreeSet<String>,
    pub pictures_failed: BTreeSet<String>,
    /// Articles whose pictures the user asked for ("Load pictures"), by article id.
    pub pictures_allowed: BTreeSet<String>,
    pub settings: Settings,
    pub list_limit: usize,
    pub start_screen: String,
    /// "Mark all as read?" is asked.
    pub confirm_mark_all: bool,
    /// "Unsubscribe?" is asked on the feed page.
    pub confirm_unsubscribe: bool,
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
            nav_open: [true, true],
            notice: String::new(),
            refreshing: 0,
            last_refresh: 0,
            pictures: BTreeSet::new(),
            pictures_asked: BTreeSet::new(),
            pictures_failed: BTreeSet::new(),
            pictures_allowed: BTreeSet::new(),
            settings,
            list_limit: LIST_PAGE,
            start_screen: args.screen.clone().unwrap_or_default(),
            confirm_mark_all: false,
            confirm_unsubscribe: false,
        }
    }

    /// The list: the view's articles that match the search (and are unread, with "Unread").
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

    /// The open article.
    fn selected_ref(&self) -> Option<ArticleRef> {
        let (feed_id, item_id) = self.selected.as_ref()?;
        let feed = self.library.feed_index(feed_id)?;
        let item = self.library.feeds[feed].items.iter().position(|i| &i.id == item_id)?;
        Some(ArticleRef { feed, item })
    }

    fn reference_of(&self, r: ArticleRef) -> Option<(String, String)> {
        let item = self.library.article(r)?;
        Some((self.library.feeds[r.feed].sub.id.clone(), item.id.clone()))
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
        let css = reader::reader_css(self.settings.font_px, self.settings.measure_px, self.settings.sepia);
        reader::article(item.body(), &item.base, self.pictures_on(item), self.settings.strip_tracking, &css)
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
    let window = kit::window_options(&kit_ref, layout, (1200.0, 760.0), (720.0, 460.0), on_window_created);
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

fn button(label: &str, id: AzString, app: &RefAny, cb: ButtonOnClickCallbackType) -> Dom {
    Button::create(label).with_on_click(app.clone(), cb).dom().with_id(id)
}

fn primary(label: &str, id: AzString, app: &RefAny, cb: ButtonOnClickCallbackType) -> Dom {
    Button::create(label)
        .with_button_type(ButtonType::Primary)
        .with_on_click(app.clone(), cb)
        .dom()
        .with_id(id)
}

fn input(value: &str, placeholder: &str, id: AzString, app: &RefAny, cb: TextInputOnTextInputCallbackType) -> Dom {
    TextInput::create()
        .with_text(value)
        .with_placeholder(placeholder)
        .with_on_text_input(app.clone(), cb)
        .dom()
        .with_id(id)
}

// ==== Navigation ====

/// A badge's text: the count, nothing for none.
fn badge(n: usize) -> String {
    if n == 0 {
        String::new()
    } else {
        n.to_string()
    }
}

/// The view of each node of the navigation pane's two groups, in the depth-first order the
/// pane's events count in (collapsed subtrees included): the order [`navigation`] adds them.
#[must_use]
pub fn navigation_views(lib: &Library) -> [Vec<View>; 2] {
    let mut articles = vec![View::All, View::Unread, View::Starred, View::Later];
    if !lib.broken().is_empty() {
        articles.push(View::Broken);
    }
    let mut feeds = vec![View::All];
    for folder in lib.folders() {
        feeds.push(View::Folder(folder.clone()));
        for f in lib.feeds.iter().filter(|f| f.sub.folder == folder) {
            feeds.push(View::Feed(f.sub.id.clone()));
        }
    }
    for f in lib.feeds.iter().filter(|f| f.sub.folder.is_empty()) {
        feeds.push(View::Feed(f.sub.id.clone()));
    }
    [articles, feeds]
}

fn navigation(s: &NewsApp, app: &RefAny) -> Dom {
    let lib = &s.library;
    let reading_articles = matches!(s.reading, Reading::Article);
    let sel = |v: &View| reading_articles && s.view == *v;
    let mut articles = TreeViewNode::create("All articles")
        .with_icon("inbox")
        .with_badge(badge(lib.unread_total()))
        .with_expanded(true)
        .with_selected(sel(&View::All))
        .with_child(
            TreeViewNode::create("Unread")
                .with_icon("mark_email_unread")
                .with_badge(badge(lib.unread_total()))
                .with_selected(sel(&View::Unread)),
        )
        .with_child(
            TreeViewNode::create("Starred")
                .with_icon("star")
                .with_badge(badge(lib.starred_count()))
                .with_selected(sel(&View::Starred)),
        )
        .with_child(
            TreeViewNode::create("Read later")
                .with_icon("bookmark")
                .with_badge(badge(lib.later_count()))
                .with_selected(sel(&View::Later)),
        );
    let broken = lib.broken();
    if !broken.is_empty() {
        articles = articles.with_child(
            TreeViewNode::create("Broken feeds")
                .with_icon("error")
                .with_badge(badge(broken.len()))
                .with_selected(sel(&View::Broken)),
        );
    }
    let feed_node = |i: usize| -> TreeViewNode {
        let f = &lib.feeds[i];
        let page_open = matches!(&s.reading, Reading::Feed(id) if *id == f.sub.id);
        TreeViewNode::create(f.name())
            .with_icon(if f.meta.error.is_empty() { "rss_feed" } else { "error" })
            .with_badge(badge(lib.unread(i)))
            .with_selected(sel(&View::Feed(f.sub.id.clone())) || page_open)
    };
    let mut feeds = TreeViewNode::create("Subscriptions").with_icon("rss_feed").with_expanded(true);
    for folder in lib.folders() {
        let mut node = TreeViewNode::create(folder.as_str())
            .with_icon("folder")
            .with_expanded(true)
            .with_badge(badge(lib.unread_in_folder(&folder)))
            .with_selected(sel(&View::Folder(folder.clone())));
        for i in (0..lib.feeds.len()).filter(|&i| lib.feeds[i].sub.folder == folder) {
            node = node.with_child(feed_node(i));
        }
        feeds = feeds.with_child(node);
    }
    for i in (0..lib.feeds.len()).filter(|&i| lib.feeds[i].sub.folder.is_empty()) {
        feeds = feeds.with_child(feed_node(i));
    }
    ShellNavigationPane::create()
        .with_header(primary("Add feed", ids::NAV_ADD, app, on_add_open))
        .with_group(
            ShellNavigationGroup::create("Articles", articles)
                .with_count(lib.unread_total())
                .with_open(s.nav_open[0]),
        )
        .with_group(
            ShellNavigationGroup::create("Feeds", feeds)
                .with_count(lib.feeds.len())
                .with_open(s.nav_open[1]),
        )
        .with_label("Feeds and folders")
        .with_on_event(app.clone(), on_nav as ShellNavigationPaneOnEventCallbackType)
        .dom()
}

// ==== The list ====

struct RowRef {
    app: RefAny,
    feed: String,
    item: String,
}

/// The list's heading for the view.
fn view_title(s: &NewsApp) -> String {
    match &s.view {
        View::All => "All articles".to_string(),
        View::Unread => "Unread".to_string(),
        View::Starred => "Starred".to_string(),
        View::Later => "Read later".to_string(),
        View::Broken => "Broken feeds".to_string(),
        View::Folder(name) => name.clone(),
        View::Feed(id) => s
            .library
            .feed_index(id)
            .map_or_else(String::new, |i| s.library.feeds[i].name().to_string()),
    }
}

/// An article's title for the list and the pane: its own, else the start of its text.
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

fn article_row(s: &NewsApp, app: &RefAny, r: ArticleRef, index: usize, now: i64, selected: Option<ArticleRef>) -> Dom {
    let lib = &s.library;
    let feed = &lib.feeds[r.feed];
    let item = &feed.items[r.item];
    let unread = !lib.is_read(r);
    let title = title_of(item);
    let mut head = vec![
        block(
            "width: 12px; flex-shrink: 0; font-size: 10px; color: #2f74d0;",
            text(if unread { "\u{25cf}" } else { "" }),
        ),
        block(
            &format!(
                "flex-grow: 1; min-width: 0px; font-size: 13px; {}",
                if unread { "font-weight: 700;" } else { "" }
            ),
            text(title.as_str()),
        ),
    ];
    if lib.is_starred(r) {
        head.push(block("padding-left: 4px; font-size: 12px;", text("\u{2605}")));
    }
    let meta = format!("{} \u{b7} {}", feed.name(), age(item.date(), now));
    column(
        &format!(
            "padding: 6px 8px; cursor: pointer; border-bottom: 1px solid rgba(128, 128, 128, 0.15); {}",
            if selected == Some(r) { "background-color: rgba(64, 128, 255, 0.18);" } else { "" }
        ),
        vec![
            row("", head),
            block("padding-left: 12px; font-size: 11px; opacity: 0.7;", text(meta)),
            block(
                "padding-left: 12px; font-size: 12px; opacity: 0.8; max-height: 2.9em; overflow: hidden;",
                text(item.excerpt.as_str()),
            ),
        ],
    )
    .with_id(ids::article(index))
    .with_class(ids::ARTICLE_ROW_CLASS)
    .with_accessibility_name(title)
    .with_callback(
        EventFilter::Hover(HoverEventFilter::MouseUp),
        RefAny::new(RowRef {
            app: app.clone(),
            feed: feed.sub.id.clone(),
            item: item.id.clone(),
        }),
        on_row,
    )
}

fn list_pane(s: &NewsApp, app: &RefAny) -> Dom {
    let now = now_secs();
    let list = s.list();
    let selected = s.selected_ref();
    let search = TextInput::create_search()
        .with_text(s.query.as_str())
        .with_placeholder("Search articles")
        .with_accessibility_name("Search articles")
        .with_on_text_input(app.clone(), on_search as TextInputOnTextInputCallbackType)
        .dom()
        .with_id(ids::LIST_SEARCH);
    let filter = Segmented::create(strs(&["All", "Unread"]))
        .with_selected_index(usize::from(s.unread_only))
        .with_on_change(app.clone(), on_filter as SegmentedOnChangeCallbackType)
        .dom()
        .with_id(ids::LIST_FILTER);
    let heading = block(
        "padding: 6px 8px 2px 8px; font-size: 12px; font-weight: 600;",
        text(format!("{} \u{b7} {}", view_title(s), list.len())),
    )
    .with_id(ids::LIST_HEADING);
    let body = if !s.loaded {
        block("padding: 16px; opacity: 0.7;", text("Reading your feeds\u{2026}"))
    } else if s.library.feeds.is_empty() {
        ShellEmptyState::create("No feeds yet")
            .with_icon("rss_feed")
            .with_detail("Add a feed by its address or a website's, or import an OPML file from another reader.")
            .with_action_label("Add feed")
            .with_on_action(app.clone(), on_add_open as ButtonOnClickCallbackType)
            .dom()
    } else if list.is_empty() {
        ShellEmptyState::create("Nothing here")
            .with_icon("search")
            .with_detail(if s.query.trim().is_empty() {
                "No articles in this view.".to_string()
            } else {
                format!("No article matches \u{201c}{}\u{201d}.", s.query.trim())
            })
            .dom()
    } else {
        let mut out = Dom::create_div()
            .with_id(ids::ARTICLE_LIST)
            .with_css("display: flex; flex-direction: column; flex-grow: 1; overflow-y: auto; min-height: 0px;");
        let shown = &list[..list.len().min(s.list_limit)];
        let today = chrono::Local::now().date_naive();
        let mut index = 0;
        for (group, members) in s.library.sections(shown, today, local_offset_secs()) {
            out.add_child(
                block(
                    "padding: 8px 8px 2px 8px; font-size: 11px; font-weight: 700; opacity: 0.8;",
                    text(group.label()),
                )
                .with_class(ids::DAY_HEADER_CLASS),
            );
            for r in members {
                out.add_child(article_row(s, app, r, index, now, selected));
                index += 1;
            }
        }
        if list.len() > shown.len() {
            out.add_child(block(
                "padding: 8px;",
                Button::create(format!("Show {} more", (list.len() - shown.len()).min(LIST_PAGE)))
                    .with_on_click(app.clone(), on_show_more as ButtonOnClickCallbackType)
                    .dom(),
            ));
        }
        out
    };
    let mut children = vec![
        row("padding: 6px 8px; gap: 6px;", vec![block("flex-grow: 1;", search), filter]),
        heading,
    ];
    if s.confirm_mark_all {
        children.push(
            row(
                "padding: 6px 8px; gap: 6px; font-size: 12px;",
                vec![
                    block("flex-grow: 1;", text(format!("Mark the {} articles here as read?", list.len()))),
                    primary("Mark as read", ids::MARK_ALL_YES, app, on_mark_all_yes),
                    button("Cancel", ids::MARK_ALL_NO, app, on_mark_all_no),
                ],
            )
            .with_id(ids::MARK_ALL_CONFIRM),
        );
    }
    children.push(body);
    column("flex-grow: 1; min-height: 0px;", children)
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
    let actions = row(
        "gap: 4px; padding: 4px 8px; flex-wrap: wrap;",
        vec![
            button("Previous", ids::READER_PREV, app, on_prev),
            button("Next", ids::READER_NEXT, app, on_next),
            block("flex-grow: 1;", Dom::create_div()),
            button("Open original", ids::READER_OPEN, app, on_open_original),
            button(
                if lib.is_starred(r) { "Unstar" } else { "Star" },
                ids::READER_STAR,
                app,
                on_star,
            ),
            button(
                if lib.is_later(r) { "Not later" } else { "Read later" },
                ids::READER_LATER,
                app,
                on_later,
            ),
            button(
                if lib.is_read(r) { "Mark unread" } else { "Mark read" },
                ids::READER_UNREAD,
                app,
                on_toggle_read,
            ),
        ],
    );
    let title = title_of(item);
    let mut pane = ReadingPane::create(title.as_str(), feed.name())
        .with_date(long_date(item.date()))
        .with_field("Reading time", format!("{} min", reader::reading_minutes(article.words)));
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
        .with_on_load_images(app.clone(), on_reading_event as ReadingPaneOnEventCallbackType)
        .with_on_link(app.clone(), on_reading_event as ReadingPaneOnEventCallbackType)
        .dom();
    column(
        "flex-grow: 1; min-height: 0px;",
        vec![actions, block("flex-grow: 1; min-height: 0px; overflow-y: auto;", pane)],
    )
}

/// A form's section title.
fn section_title(title: &str) -> Dom {
    block(
        "padding: 12px 0px 4px 0px; font-size: 11px; font-weight: 700; opacity: 0.7;",
        text(title.to_uppercase()),
    )
}

fn problem_line(problem: &str, id: AzString) -> Dom {
    block("color: #b3261e; padding: 4px 0px; font-size: 13px;", text(problem)).with_id(id)
}

struct PickRef {
    app: RefAny,
    index: usize,
}

fn add_feed_view(app: &RefAny, st: &AddFeed) -> Dom {
    let mut children = vec![
        block("font-size: 18px; font-weight: 600; padding-bottom: 8px;", text("Add a feed")),
        kit::row(
            "Website or feed",
            row(
                "gap: 6px; flex-grow: 1;",
                vec![
                    block(
                        "flex-grow: 1;",
                        input(&st.input, "https://example.org", ids::ADD_URL, app, on_add_input),
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
            let label = format!("{name} \u{b7} {} \u{b7} {} articles", c.feed.format.label(), c.feed.items.len());
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
        actions.push(primary("Subscribe", ids::ADD_SUBSCRIBE, app, on_add_subscribe));
    }
    actions.push(button("Cancel", ids::ADD_CANCEL, app, on_leave));
    children.push(row("gap: 6px; padding-top: 12px;", actions));
    column("padding: 16px; flex-grow: 1; min-height: 0px; overflow-y: auto;", children).with_id(ids::ADD_FEED)
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
            row(
                "gap: 6px; flex-grow: 1;",
                vec![
                    block(
                        "flex-grow: 1;",
                        input(&st.path, "/path/to/subscriptions.opml", ids::OPML_PATH, app, on_import_path),
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
        let mut rows = Dom::create_div().with_id(ids::OPML_ROWS).with_css("display: flex; flex-direction: column;");
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
            let folder = if r.sub.folder.is_empty() { String::new() } else { format!(" \u{b7} {}", r.sub.folder) };
            rows.add_child(
                row(
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
    children.push(row("gap: 6px; padding-top: 12px;", actions));
    column("padding: 16px; flex-grow: 1; min-height: 0px; overflow-y: auto;", children).with_id(ids::OPML_IMPORT)
}

fn feed_page(s: &NewsApp, app: &RefAny, index: usize) -> Dom {
    let f = &s.library.feeds[index];
    let checked = if f.meta.checked == 0 {
        "never".to_string()
    } else {
        format!("{} ago", age(f.meta.checked, now_secs()))
    };
    let mut children = vec![
        block("font-size: 18px; font-weight: 600; padding-bottom: 8px;", text(f.name())),
        kit::row("Name", input(&f.sub.title, "The feed's own title", ids::FEED_TITLE, app, on_feed_title)),
        kit::row("Folder", input(&f.sub.folder, "No folder", ids::FEED_FOLDER, app, on_feed_folder)),
        kit::row("Address", text(f.sub.url.as_str())),
        kit::row("Website", text(f.meta.site.as_str())),
        kit::row("Format", text(f.meta.kind.as_str())),
        kit::row("Articles", text(format!("{} ({} unread)", f.items.len(), s.library.unread(index)))),
        kit::row("Last asked", text(checked)),
    ];
    if !f.meta.error.is_empty() {
        children.push(problem_line(&format!("The last refresh failed: {}", f.meta.error), ids::ADD_PROBLEM));
    }
    let mut actions = vec![
        primary("Refresh now", ids::FEED_REFRESH, app, on_feed_refresh),
        button("Show its articles", ids::FEED_PAGE, app, on_feed_articles),
    ];
    if s.confirm_unsubscribe {
        actions.push(
            Button::create("Unsubscribe and delete its articles")
                .with_button_type(ButtonType::Danger)
                .with_on_click(app.clone(), on_unsubscribe_confirmed as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::FEED_UNSUBSCRIBE_CONFIRM),
        );
    } else {
        actions.push(button("Unsubscribe", ids::FEED_UNSUBSCRIBE, app, on_unsubscribe));
    }
    children.push(row("gap: 6px; padding-top: 12px; flex-wrap: wrap;", actions));
    column("padding: 16px; flex-grow: 1; min-height: 0px; overflow-y: auto;", children)
}

fn reading_pane(s: &NewsApp, app: &RefAny) -> Dom {
    match &s.reading {
        Reading::AddFeed(st) => add_feed_view(app, st),
        Reading::Import(st) => import_view(app, st),
        Reading::Feed(id) => match s.library.feed_index(id) {
            Some(i) => feed_page(s, app, i),
            None => empty_reading(app),
        },
        Reading::Article => match s.selected_ref() {
            Some(r) => article_view(s, app, r),
            None => empty_reading(app),
        },
    }
}

// ==== Toolbar, status bar, settings, the window ====

fn toolbar(s: &NewsApp, app: &RefAny) -> Dom {
    let tool = |label: &str, icon: &str, id: AzString, cb: ButtonOnClickCallbackType| {
        Button::create(label).with_icon(icon).with_on_click(app.clone(), cb).dom().with_id(id)
    };
    row(
        "gap: 4px; padding: 4px 8px;",
        vec![
            tool(
                if s.refreshing > 0 { "Refreshing\u{2026}" } else { "Refresh" },
                "refresh",
                ids::TOOLBAR_REFRESH,
                on_refresh,
            ),
            tool("Add feed", "add", ids::TOOLBAR_ADD, on_add_open),
            tool("Import", "file_upload", ids::TOOLBAR_IMPORT, on_import_open),
            tool("Export", "file_download", ids::TOOLBAR_EXPORT, on_export),
            tool("Mark all as read", "done_all", ids::TOOLBAR_MARK_ALL, on_mark_all),
            block("flex-grow: 1;", Dom::create_div()),
            tool("Settings", "settings", ids::TOOLBAR_SETTINGS, on_open_settings),
        ],
    )
}

fn status_bar(s: &NewsApp, _app: &RefAny) -> Dom {
    let mut segments = Vec::new();
    segments.push(StatusBarSegment::create(if s.refreshing > 0 {
        format!("Refreshing \u{2013} {} feed(s) to go", s.refreshing)
    } else if s.last_refresh > 0 {
        format!("Updated {} ago", age(s.last_refresh, now_secs()))
    } else {
        "Not refreshed yet".to_string()
    }));
    segments.push(StatusBarSegment::create(format!("{} unread", s.library.unread_total())));
    if !s.notice.is_empty() {
        segments.push(StatusBarSegment::create(s.notice.as_str()));
    }
    StatusBar::create(segments).dom().with_id(ids::NEWS_STATUS)
}

/// A settings choice of several labels.
fn choice(labels: Vec<String>, selected: usize, app: &RefAny, cb: SegmentedOnChangeCallbackType, id: AzString) -> Dom {
    Segmented::create(StringVec::from_vec(labels.into_iter().map(AzString::from).collect()))
        .with_selected_index(selected)
        .with_on_change(app.clone(), cb)
        .dom()
        .with_id(id)
}

fn switch(on: bool, app: &RefAny, cb: SwitchOnToggleCallbackType, id: AzString) -> Dom {
    Switch::create(on).with_on_toggle(app.clone(), cb).dom().with_id(id)
}

fn settings_sections(s: &NewsApp, app: &RefAny) -> Vec<AppSection> {
    let st = &s.settings;
    let font = FONT_SIZES.iter().position(|p| *p == st.font_px).unwrap_or(2);
    let measure = MEASURES.iter().position(|(_, px)| *px == st.measure_px).unwrap_or(1);
    let pictures = Pictures::ALL.iter().position(|p| *p == st.pictures).unwrap_or(1);
    let every = REFRESH_EVERY.iter().position(|(_, m)| *m == st.refresh_minutes).unwrap_or(2);
    let keep = KEEP.iter().position(|(_, d)| *d == st.keep_days).unwrap_or(1);
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
                        "A refresh asks each feed with what it said last time (ETag, Last-Modified): a feed \
                         without news costs nothing. Your feeds, articles and marks are files in {}.",
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
            vec![kit::title_row(SPEC.name), kit::settings_page(&s.kit, settings_sections(s, &app))],
        )
    } else {
        PimShell::create(navigation(s, &app), list_pane(s, &app), reading_pane(s, &app))
            .with_list_label("Articles")
            .office_shell()
            .with_title_row(kit::title_row(SPEC.name))
            .with_ribbon(toolbar(s, &app))
            .with_status_bar(status_bar(s, &app))
            .dom()
    };
    let root = column("flex-grow: 1; min-height: 0px;", vec![content]);
    // The theme scope's own body: no UA margin, the window's full height.
    ShellThemeScope::create(root)
        .with_accent(ShellThemeAccent::Clay)
        .body()
        .with_callback(EventFilter::Window(WindowEventFilter::VirtualKeyDown), app, on_key)
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
        assert_eq!(age(now - 40 * DAY, now).len(), 10, "a date: {}", age(now - 40 * DAY, now));
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
        assert_eq!(Settings::read(&|_: &str| -> Option<String> { None }), Settings::default());
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
