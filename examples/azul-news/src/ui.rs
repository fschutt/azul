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
