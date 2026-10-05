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

use crate::{
    feed::Item,
    fetch::{Candidate, Fetched},
    ids,
    jobs::{self, FindEvent, PictureEvent, RefreshEvent, RefreshJob},
    library::{ArticleRef, Library, View, DAY},
    links,
    opml::{self, Subscription},
    reader, sample, store,
};
use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, ReadingPaneOnEventCallbackType,
        SegmentedOnChangeCallbackType, ShellNavigationPaneOnEventCallbackType,
        SwitchOnToggleCallbackType, TextInputOnTextInputCallbackType, TimerCallbackInfo,
        TimerCallbackReturn, ToolbarOnEventCallbackType,
    },
    dialog::{FileDialog, FileOpenResult},
    image::ImageRef,
    option::{OptionFileTypeList, OptionString},
    prelude::*,
    shells::{
        PimShell, ShellEmptyState, ShellNavigationGroup, ShellNavigationPane,
        ShellNavigationPaneEvent, ShellNavigationPaneEventKind, ShellThemeAccent, ShellThemeScope,
    },
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    vec::StringVec,
    widgets::{
        ButtonType, CheckBox, CheckBoxState, InfoBar, OnTextInputReturn, ReadingPane,
        ReadingPaneEvent, ReadingPaneEventKind, Segmented, SegmentedState, StatusBar,
        StatusBarSegment, Switch, SwitchState, TextInputState, TextInputValid, Toolbar,
        ToolbarEvent, ToolbarEventKind, ToolbarItem, TreeViewNode,
    },
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec},
    files::{FileJob, FileOutcome},
    shortcuts::Shortcut,
    ui::{self as kit, AppSection},
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
/// The subscription list written on the way out: the window closes when it landed.
const TAG_CLOSING: u64 = 5;

/// Articles the list shows before "Show more".
const LIST_PAGE: usize = 300;

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
            .map(|d| {
                d.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d")
                    .to_string()
            })
            .unwrap_or_default()
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
    /// The feed page changed the subscription list (written when the page is left, or when
    /// the window is asked to close).
    pub list_dirty: bool,
    /// The window waits for the subscription list to land before it closes.
    pub closing: bool,
    /// The "Refresh every ..." timer, while one runs.
    pub refresh_timer: Option<TimerId>,
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
            list_dirty: false,
            closing: false,
            refresh_timer: None,
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
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; {css}"
        ))
        .with_children(DomVec::from_vec(children))
}

fn button(label: &str, id: AzString, app: &RefAny, cb: ButtonOnClickCallbackType) -> Dom {
    Button::create(label)
        .with_on_click(app.clone(), cb)
        .dom()
        .with_id(id)
}

fn primary(label: &str, id: AzString, app: &RefAny, cb: ButtonOnClickCallbackType) -> Dom {
    Button::create(label)
        .with_button_type(ButtonType::Primary)
        .with_on_click(app.clone(), cb)
        .dom()
        .with_id(id)
}

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
            .with_icon(if f.meta.error.is_empty() {
                "rss_feed"
            } else {
                "error"
            })
            .with_badge(badge(lib.unread(i)))
            .with_selected(sel(&View::Feed(f.sub.id.clone())) || page_open)
    };
    let mut feeds = TreeViewNode::create("Subscriptions")
        .with_icon("rss_feed")
        .with_expanded(true);
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
        .with_on_event(
            app.clone(),
            on_nav as ShellNavigationPaneOnEventCallbackType,
        )
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

fn article_row(
    s: &NewsApp,
    app: &RefAny,
    r: ArticleRef,
    index: usize,
    now: i64,
    selected: Option<ArticleRef>,
) -> Dom {
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
        head.push(block(
            "padding-left: 4px; font-size: 12px;",
            text("\u{2605}"),
        ));
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

// TODO(WIDGETS9A): IconGrid - the plan's magazine mode (a grid of cards with the articles'
// pictures, `List | Cards`) comes with azul's IconGrid; today the list is the one mode.
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
        block(
            "padding: 16px; opacity: 0.7;",
            text("Reading your feeds\u{2026}"),
        )
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
                Button::create(format!(
                    "Show {} more",
                    (list.len() - shown.len()).min(LIST_PAGE)
                ))
                .with_on_click(app.clone(), on_show_more as ButtonOnClickCallbackType)
                .dom(),
            ));
        }
        out
    };
    let mut children = vec![
        row(
            "padding: 6px 8px; gap: 6px;",
            vec![block("flex-grow: 1;", search), filter],
        ),
        heading,
    ];
    if s.confirm_mark_all {
        children.push(
            row(
                "padding: 6px 8px; gap: 6px; font-size: 12px;",
                vec![
                    block(
                        "flex-grow: 1;",
                        text(format!("Mark the {} articles here as read?", list.len())),
                    ),
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
                if lib.is_later(r) {
                    "Not later"
                } else {
                    "Read later"
                },
                ids::READER_LATER,
                app,
                on_later,
            ),
            button(
                if lib.is_read(r) {
                    "Mark unread"
                } else {
                    "Mark read"
                },
                ids::READER_UNREAD,
                app,
                on_toggle_read,
            ),
        ],
    );
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
    column(
        "flex-grow: 1; min-height: 0px;",
        vec![
            actions,
            block("flex-grow: 1; min-height: 0px; overflow-y: auto;", pane),
        ],
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
    block(
        "color: #b3261e; padding: 4px 0px; font-size: 13px;",
        text(problem),
    )
    .with_id(id)
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
            row(
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
    children.push(row("gap: 6px; padding-top: 12px;", actions));
    column(
        "padding: 16px; flex-grow: 1; min-height: 0px; overflow-y: auto;",
        children,
    )
    .with_id(ids::ADD_FEED)
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
    column(
        "padding: 16px; flex-grow: 1; min-height: 0px; overflow-y: auto;",
        children,
    )
    .with_id(ids::OPML_IMPORT)
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
    children.push(row(
        "gap: 6px; padding-top: 12px; flex-wrap: wrap;",
        actions,
    ));
    column(
        "padding: 16px; flex-grow: 1; min-height: 0px; overflow-y: auto;",
        children,
    )
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

/// The toolbar in the ribbon row: azul's `Toolbar` (roving focus, the "more" menu). Each tool's
/// `id` is its DOM-id name from [`ids`]: what [`on_toolbar`] matches.
fn toolbar(s: &NewsApp, app: &RefAny) -> Dom {
    let tool = |id: AzString, label: &str, icon: &str| {
        ToolbarItem::create_button(id, label, icon).with_show_label(true)
    };
    let refresh = if s.refreshing > 0 {
        "Refreshing\u{2026}"
    } else {
        "Refresh"
    };
    let items = vec![
        tool(ids::TOOLBAR_REFRESH, refresh, "refresh"),
        tool(ids::TOOLBAR_ADD, "Add feed", "add"),
        tool(ids::TOOLBAR_IMPORT, "Import", "file_upload"),
        tool(ids::TOOLBAR_EXPORT, "Export", "file_download"),
        tool(ids::TOOLBAR_MARK_ALL, "Mark all as read", "done_all"),
        ToolbarItem::create_spacer(),
        tool(ids::TOOLBAR_SETTINGS, "Settings", "settings"),
    ];
    block(
        "padding: 4px 8px;",
        Toolbar::create("News")
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
    let command: ButtonOnClickCallbackType = if id == ids::TOOLBAR_REFRESH.as_str() {
        on_refresh
    } else if id == ids::TOOLBAR_ADD.as_str() {
        on_add_open
    } else if id == ids::TOOLBAR_IMPORT.as_str() {
        on_import_open
    } else if id == ids::TOOLBAR_EXPORT.as_str() {
        on_export
    } else if id == ids::TOOLBAR_MARK_ALL.as_str() {
        on_mark_all
    } else if id == ids::TOOLBAR_SETTINGS.as_str() {
        on_open_settings
    } else {
        return Update::DoNothing;
    };
    command(data, info)
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
    segments.push(StatusBarSegment::create(format!(
        "{} unread",
        s.library.unread_total()
    )));
    if !s.notice.is_empty() {
        segments.push(StatusBarSegment::create(s.notice.as_str()));
    }
    StatusBar::create(segments).dom().with_id(ids::NEWS_STATUS)
}

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
            vec![
                kit::title_row(SPEC.name),
                kit::settings_page(&s.kit, settings_sections(s, &app)),
            ],
        )
    } else {
        PimShell::create(
            navigation(s, &app),
            list_pane(s, &app),
            reading_pane(s, &app),
        )
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
    s.refreshing += jobs.len();
    jobs::spawn_refresh(info, jobs, app.clone(), on_refresh_event);
}

fn all_feeds(s: &NewsApp) -> Vec<usize> {
    (0..s.library.feeds.len()).collect()
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

/// "Refresh every ...": all feeds, unless a refresh is running.
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
    let feeds = all_feeds(&s);
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
        "feed" => {
            if let Some(f) = s.library.feeds.first() {
                s.reading = Reading::Feed(f.sub.id.clone());
            }
        }
        _ => {
            // The newest article opens (and so is read), as in Outlook's reading pane.
            if let Some(first) = s.list().first().copied() {
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
        let feeds = all_feeds(s);
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

// ==== Callbacks: navigation and the list ====

fn set_view(s: &mut NewsApp, view: View) {
    s.view = view;
    s.list_limit = LIST_PAGE;
    s.confirm_mark_all = false;
    println!("AZNEWS_VIEW {}", s.list().len());
}

extern "C" fn on_nav(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: ShellNavigationPaneEvent,
) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| match event.kind {
        ShellNavigationPaneEventKind::GroupToggled => {
            if event.group < s.nav_open.len() {
                s.nav_open[event.group] = event.expand;
            }
        }
        ShellNavigationPaneEventKind::NodeClicked => {
            let views = navigation_views(&s.library);
            if let Some(view) = views
                .get(event.group)
                .and_then(|v| v.get(event.index))
                .cloned()
            {
                leave_form(s, info, handle);
                set_view(s, view);
            }
        }
        _ => {}
    })
}

extern "C" fn on_row(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, feed, item)) = data
        .downcast_ref::<RowRef>()
        .map(|r| (r.app.clone(), r.feed.clone(), r.item.clone()))
    else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, info, handle| {
        let Some(f) = s.library.feed_index(&feed) else {
            return;
        };
        let Some(i) = s.library.feeds[f].items.iter().position(|x| x.id == item) else {
            return;
        };
        leave_form(s, info, handle);
        select(s, info, handle, ArticleRef { feed: f, item: i });
    })
}

extern "C" fn on_search(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let query = state.get_text().as_str().to_string();
    let update = with_app(&mut data, &mut info, |s, _info, _| {
        s.query = query;
        s.list_limit = LIST_PAGE;
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
        s.list_limit = LIST_PAGE;
        println!("AZNEWS_VIEW {}", s.list().len());
    })
}

extern "C" fn on_show_more(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        s.list_limit += LIST_PAGE
    })
}

extern "C" fn on_mark_all(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        s.confirm_mark_all = true
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
        let unread = list.iter().filter(|r| !s.library.is_read(**r)).count();
        let changed = s.library.mark_all_read(&list);
        let jobs: Vec<FileJob> = changed
            .iter()
            .filter_map(|&f| s.library.feeds.get(f).map(store::state_job))
            .collect();
        write_files(s, info, handle, jobs, TAG_WRITE);
        println!("AZNEWS_MARKED_ALL {unread}");
        s.notice = format!("{unread} article(s) marked as read");
    })
}

// ==== Callbacks: the article ====

/// The open article's place in the list and the next / previous one.
fn step(s: &mut NewsApp, info: &mut CallbackInfo, app: &RefAny, forward: bool) {
    let list = s.list();
    if let Some(r) = s.library.next(&list, s.selected_ref(), forward) {
        select(s, info, app, r);
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
        let Some(link) = s
            .selected_ref()
            .and_then(|r| s.library.article(r))
            .map(|i| i.link.clone())
        else {
            return;
        };
        let link = if s.settings.strip_tracking {
            links::strip_tracking(&link)
        } else {
            link
        };
        if let Err(e) = azul_appkit::files::open_external(&link) {
            s.notice = e;
        }
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
        start_refresh(s, info, handle, added);
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

// ==== Callbacks: a feed's page ====

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
        if s.library.unsubscribe(&id).is_some() {
            println!("AZNEWS_UNSUBSCRIBED {id}");
            write_files(s, info, handle, store::delete_jobs(&id), TAG_WRITE);
            save_list(s, info, handle);
        }
        if s.selected.as_ref().is_some_and(|(feed, _)| *feed == id) {
            s.selected = None;
        }
        s.confirm_unsubscribe = false;
        s.reading = Reading::Article;
        set_view(s, View::All);
    })
}

// ==== Callbacks: toolbar, settings, keys ====

extern "C" fn on_refresh(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        if s.refreshing == 0 {
            let feeds = all_feeds(s);
            start_refresh(s, info, handle, feeds);
        }
    })
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
    // Plain keys are the list's only while no field has the focus (typing "j" into the search
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
        assert_eq!(
            Settings::read(&|_: &str| -> Option<String> { None }),
            Settings::default()
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
