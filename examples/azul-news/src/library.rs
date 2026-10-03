//! The subscriptions, their articles and their marks - what the window shows, as plain data.
//!
//! - the views of the navigation pane ([`View`]): all articles, unread, starred, read later, a
//!   folder, one feed, the feeds that failed;
//! - the list: the view's articles newest first, filtered by the search box (every word, in any
//!   case, diacritics folded: azul-pim's one search), in Outlook's date groups (azul-pim's
//!   [`DateGroup`]);
//! - the unread counts the navigation pane shows;
//! - a refresh merged in ([`Library::merge`]): an article keeps the time AzNews first saw it; an
//!   article the feed dropped stays while it is inside the "keep" window, and always when it is
//!   starred or kept for later; at most [`MAX_ITEMS`] per feed; the read marks of articles gone
//!   are forgotten.

use std::collections::{BTreeMap, BTreeSet};

use azul_pim::{
    dates::{date_group, DateGroup},
    search::Query,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::{
    feed::{Feed, Item},
    opml::Subscription,
    state::ReadState,
};

/// The `format` of a feed's `feed.json`.
pub const META_FORMAT: &str = "aznews.feed";
/// The `feed.json` version this AzNews writes.
pub const META_VERSION: u64 = 1;
/// The most articles kept per feed (starred and saved ones on top of that).
pub const MAX_ITEMS: usize = 500;
/// Seconds in a day.
pub const DAY: i64 = 86_400;

/// What AzNews knows about a feed besides its articles: `news/feeds/<id>/feed.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FeedMeta {
    pub format: String,
    pub version: u64,
    /// The feed's own title (the subscription's title is the user's name for it).
    pub title: String,
    pub site: String,
    pub icon: String,
    /// "RSS 2.0", "Atom", "JSON Feed".
    pub kind: String,
    /// The validators of the last answer, for the next conditional GET.
    pub etag: String,
    pub last_modified: String,
    /// When it was last asked (seconds since 1970; 0: never).
    pub checked: i64,
    /// When new articles last came.
    pub updated: i64,
    /// The last HTTP status (0: no answer).
    pub status: u16,
    /// Why the last refresh failed ("": it did not).
    pub error: String,
}

impl FeedMeta {
    /// The file's text.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut file = self.clone();
        file.format = META_FORMAT.to_string();
        file.version = META_VERSION;
        serde_json::to_string_pretty(&file).unwrap_or_default()
    }

    /// A `feed.json` (`None` when it cannot be read).
    #[must_use]
    pub fn from_json(text: &str) -> Option<FeedMeta> {
        serde_json::from_str(text).ok()
    }
}

/// One subscribed feed with everything AzNews keeps of it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FeedData {
    pub sub: Subscription,
    pub meta: FeedMeta,
    /// Newest first.
    pub items: Vec<Item>,
    pub state: ReadState,
}

impl FeedData {
    /// The name the list and the navigation show: the user's, else the feed's, else its address.
    #[must_use]
    pub fn name(&self) -> &str {
        if !self.sub.title.trim().is_empty() {
            &self.sub.title
        } else if !self.meta.title.trim().is_empty() {
            &self.meta.title
        } else {
            &self.sub.url
        }
    }
}

/// What the navigation pane selects.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    All,
    Unread,
    Starred,
    Later,
    Folder(String),
    /// One feed, by its id.
    Feed(String),
    /// The feeds whose last refresh failed.
    Broken,
}

/// An article: its feed's index and its index in that feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ArticleRef {
    pub feed: usize,
    pub item: usize,
}

/// Every subscribed feed, in the subscription list's order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Library {
    pub feeds: Vec<FeedData>,
}

impl Library {
    #[must_use]
    pub fn feed_index(&self, _id: &str) -> Option<usize> {
        None
    }

    /// The folders, in the order their first feed comes.
    #[must_use]
    pub fn folders(&self) -> Vec<String> {
        Vec::new()
    }

    /// The unread articles of one feed.
    #[must_use]
    pub fn unread(&self, _feed: usize) -> usize {
        0
    }

    #[must_use]
    pub fn unread_total(&self) -> usize {
        0
    }

    #[must_use]
    pub fn unread_in_folder(&self, _folder: &str) -> usize {
        0
    }

    #[must_use]
    pub fn starred_count(&self) -> usize {
        0
    }

    #[must_use]
    pub fn later_count(&self) -> usize {
        0
    }

    /// The feeds whose last refresh failed.
    #[must_use]
    pub fn broken(&self) -> Vec<usize> {
        Vec::new()
    }

    #[must_use]
    pub fn article(&self, _r: ArticleRef) -> Option<&Item> {
        None
    }

    /// The view's articles that match `query`, newest first.
    #[must_use]
    pub fn list(&self, _view: &View, _query: &str) -> Vec<ArticleRef> {
        Vec::new()
    }

    /// `refs` (newest first) in their date groups, `today` and the dates in the local time
    /// `offset_secs` east of UTC.
    #[must_use]
    pub fn sections(&self, _refs: &[ArticleRef], _today: NaiveDate, _offset_secs: i64) -> Vec<(DateGroup, Vec<ArticleRef>)> {
        let _unused = date_group;
        Vec::new()
    }

    #[must_use]
    pub fn is_read(&self, _r: ArticleRef) -> bool {
        false
    }

    /// Marks an article read or unread; whether that changed anything.
    pub fn set_read(&mut self, _r: ArticleRef, _read: bool) -> bool {
        false
    }

    #[must_use]
    pub fn is_starred(&self, _r: ArticleRef) -> bool {
        false
    }

    /// Stars or unstars an article; its new state.
    pub fn toggle_star(&mut self, _r: ArticleRef) -> bool {
        false
    }

    #[must_use]
    pub fn is_later(&self, _r: ArticleRef) -> bool {
        false
    }

    /// Keeps an article for later, or not; its new state.
    pub fn toggle_later(&mut self, _r: ArticleRef) -> bool {
        false
    }

    /// Marks the articles read; the feeds whose marks changed (their files to write).
    pub fn mark_all_read(&mut self, _refs: &[ArticleRef]) -> Vec<usize> {
        Vec::new()
    }

    /// A refresh of feed `feed` merged in (see the module documentation); how many articles
    /// are new.
    pub fn merge(&mut self, _feed: usize, _parsed: Feed, _now: i64, _keep_days: u32) -> usize {
        let _unused: (BTreeMap<String, i64>, BTreeSet<String>, Option<Query>) = (BTreeMap::new(), BTreeSet::new(), None);
        0
    }

    /// Adds a feed (at the end); its index. A feed whose address is subscribed already is not
    /// added twice: that one's index.
    pub fn subscribe(&mut self, _sub: Subscription) -> usize {
        0
    }

    /// Removes the feed with this id; what it was.
    pub fn unsubscribe(&mut self, _id: &str) -> Option<FeedData> {
        None
    }

    /// The subscription list (for `subscriptions.opml`).
    #[must_use]
    pub fn subscriptions(&self) -> Vec<Subscription> {
        Vec::new()
    }

    /// The article after (or before) `current` in `list`; the first (last) one without one.
    #[must_use]
    pub fn next(&self, _list: &[ArticleRef], _current: Option<ArticleRef>, _forward: bool) -> Option<ArticleRef> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Thursday, 1 October 2026, 12:00 UTC.
    const NOW: i64 = 1_790_856_000;

    fn item(id: &str, title: &str, date: i64) -> Item {
        Item {
            id: id.to_string(),
            title: title.to_string(),
            link: format!("https://example.org/{id}"),
            published: Some(date),
            excerpt: format!("about {title}"),
            ..Item::default()
        }
    }

    fn feed_data(id: &str, title: &str, folder: &str, items: Vec<Item>) -> FeedData {
        FeedData {
            sub: Subscription {
                id: id.to_string(),
                title: title.to_string(),
                url: format!("https://{id}.example.org/feed"),
                site: String::new(),
                folder: folder.to_string(),
            },
            meta: FeedMeta::default(),
            items,
            state: ReadState::default(),
        }
    }

    fn library() -> Library {
        Library {
            feeds: vec![
                feed_data(
                    "a",
                    "Example Weekly",
                    "Tech",
                    vec![item("a1", "The quiet return of RSS", NOW - 3_600), item("a2", "Older essay", NOW - 3 * DAY)],
                ),
                feed_data("b", "Rust Blog", "Tech", vec![item("b1", "Rust 2026 survey results", NOW - 2 * 3_600)]),
                feed_data("c", "Bakery News", "Local", vec![item("c1", "New bakery opens on Main St.", NOW - DAY - 60)]),
            ],
        }
    }

    fn r(feed: usize, item: usize) -> ArticleRef {
        ArticleRef { feed, item }
    }

    #[test]
    fn the_list_is_newest_first_across_feeds() {
        let lib = library();
        assert_eq!(lib.list(&View::All, ""), vec![r(0, 0), r(1, 0), r(2, 0), r(0, 1)]);
        assert_eq!(lib.list(&View::Folder("Tech".into()), ""), vec![r(0, 0), r(1, 0), r(0, 1)]);
        assert_eq!(lib.list(&View::Feed("c".into()), ""), vec![r(2, 0)]);
        assert_eq!(lib.folders(), vec!["Tech".to_string(), "Local".to_string()]);
        assert_eq!(lib.feed_index("b"), Some(1));
        assert_eq!(lib.feed_index("zzz"), None);
    }

    #[test]
    fn the_search_box_finds_every_word_in_any_case() {
        let lib = library();
        assert_eq!(lib.list(&View::All, "rss QUIET"), vec![r(0, 0)]);
        assert_eq!(lib.list(&View::All, "rust blog"), vec![r(1, 0)], "the feed's name counts too");
        assert_eq!(lib.list(&View::All, "nothing like this"), Vec::<ArticleRef>::new());
    }

    #[test]
    fn read_marks_change_the_counts_and_the_unread_view() {
        let mut lib = library();
        assert_eq!(lib.unread_total(), 4);
        assert_eq!(lib.unread_in_folder("Tech"), 3);
        assert!(lib.set_read(r(0, 0), true));
        assert!(!lib.set_read(r(0, 0), true));
        assert!(lib.is_read(r(0, 0)));
        assert_eq!(lib.unread(0), 1);
        assert_eq!(lib.unread_total(), 3);
        assert_eq!(lib.list(&View::Unread, ""), vec![r(1, 0), r(2, 0), r(0, 1)]);
        assert_eq!(lib.mark_all_read(&[r(1, 0), r(0, 1), r(0, 0)]), vec![0, 1], "the feeds that changed");
        assert_eq!(lib.unread_total(), 1);
    }

    #[test]
    fn starred_and_later_views_and_counts() {
        let mut lib = library();
        assert!(lib.toggle_star(r(2, 0)));
        assert!(lib.toggle_later(r(0, 1)));
        assert!(lib.is_starred(r(2, 0)) && lib.is_later(r(0, 1)));
        assert_eq!(lib.starred_count(), 1);
        assert_eq!(lib.later_count(), 1);
        assert_eq!(lib.list(&View::Starred, ""), vec![r(2, 0)]);
        assert_eq!(lib.list(&View::Later, ""), vec![r(0, 1)]);
    }

    #[test]
    fn the_list_falls_into_outlooks_date_groups_in_local_time() {
        let lib = library();
        let today = NaiveDate::from_ymd_opt(2026, 10, 1).expect("a date");
        let list = lib.list(&View::All, "");
        let sections = lib.sections(&list, today, 0);
        let groups: Vec<DateGroup> = sections.iter().map(|(g, _)| *g).collect();
        assert_eq!(groups, vec![DateGroup::Today, DateGroup::Yesterday, DateGroup::Weekday(chrono::Weekday::Mon)]);
        assert_eq!(sections[0].1, vec![r(0, 0), r(1, 0)]);
        // 13 hours west of UTC the article of 11:00 UTC is still today, the one of yesterday
        // 11:59 UTC too is yesterday.
        let west = lib.sections(&list, NaiveDate::from_ymd_opt(2026, 9, 30).expect("a date"), -13 * 3_600);
        assert_eq!(west[0].0, DateGroup::Today);
    }

    #[test]
    fn a_refresh_keeps_the_marks_and_the_first_seen_time_and_counts_what_is_new() {
        let mut lib = library();
        lib.feeds[0].items[0].seen = NOW - 3_600;
        lib.set_read(r(0, 0), true);
        lib.toggle_star(r(0, 1));
        let parsed = Feed {
            title: "Example Weekly (the feed)".to_string(),
            items: vec![
                item("a3", "Brand new", NOW - 60),
                item("a1", "The quiet return of RSS (edited)", NOW - 3_600),
            ],
            ..Feed::default()
        };
        assert_eq!(lib.merge(0, parsed, NOW, 30), 1, "one new article");
        let ids: Vec<&str> = lib.feeds[0].items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, vec!["a3", "a1", "a2"], "newest first; the dropped article stays (starred, and inside 30 days)");
        assert_eq!(lib.feeds[0].items[1].title, "The quiet return of RSS (edited)");
        assert_eq!(lib.feeds[0].items[1].seen, NOW - 3_600, "first seen stays");
        assert_eq!(lib.feeds[0].items[0].seen, NOW);
        assert!(lib.feeds[0].state.is_read("a1"), "the read mark stays");
        assert!(lib.feeds[0].state.is_starred("a2"));
        assert_eq!(lib.feeds[0].meta.title, "Example Weekly (the feed)");
        assert_eq!(lib.feeds[0].meta.updated, NOW);
        assert_eq!(lib.feeds[0].meta.error, "");
    }

    #[test]
    fn an_article_the_feed_dropped_goes_after_the_keep_window_unless_it_is_starred() {
        let mut lib = library();
        lib.feeds[0].items[1].published = Some(NOW - 40 * DAY);
        lib.set_read(r(0, 1), true);
        assert_eq!(lib.merge(0, Feed { items: vec![item("a1", "x", NOW - 3_600)], ..Feed::default() }, NOW, 30), 0);
        let ids: Vec<&str> = lib.feeds[0].items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, vec!["a1"]);
        assert!(!lib.feeds[0].state.is_read("a2"), "its read mark is forgotten");
    }

    #[test]
    fn subscribing_twice_to_one_address_adds_one_feed_and_unsubscribing_removes_it() {
        let mut lib = library();
        let sub = Subscription {
            id: "d".into(),
            title: "New".into(),
            url: "https://d.example.org/feed".into(),
            ..Subscription::default()
        };
        assert_eq!(lib.subscribe(sub.clone()), 3);
        assert_eq!(lib.subscribe(Subscription { id: "e".into(), ..sub }), 3);
        assert_eq!(lib.feeds.len(), 4);
        assert_eq!(lib.subscriptions().len(), 4);
        assert_eq!(lib.unsubscribe("b").map(|f| f.sub.title), Some("Rust Blog".to_string()));
        assert_eq!(lib.feed_index("d"), Some(2));
    }

    #[test]
    fn next_and_previous_walk_the_list() {
        let lib = library();
        let list = lib.list(&View::All, "");
        assert_eq!(lib.next(&list, None, true), Some(r(0, 0)));
        assert_eq!(lib.next(&list, Some(r(0, 0)), true), Some(r(1, 0)));
        assert_eq!(lib.next(&list, Some(r(0, 1)), true), None, "the end");
        assert_eq!(lib.next(&list, Some(r(1, 0)), false), Some(r(0, 0)));
        assert_eq!(lib.next(&list, None, false), Some(r(0, 1)));
    }

    #[test]
    fn a_failed_feed_is_in_the_broken_view() {
        let mut lib = library();
        lib.feeds[2].meta.error = "HTTP 404".into();
        assert_eq!(lib.broken(), vec![2]);
        assert_eq!(lib.list(&View::Broken, ""), vec![r(2, 0)]);
    }

    #[test]
    fn the_feed_file_round_trips() {
        let meta = FeedMeta { etag: "\"abc\"".into(), checked: NOW, status: 200, ..FeedMeta::default() };
        let text = meta.to_json();
        assert!(text.contains("aznews.feed"));
        let back = FeedMeta::from_json(&text).expect("its own file");
        assert_eq!(back.etag, "\"abc\"");
        assert_eq!(back.checked, NOW);
        assert_eq!(FeedMeta::from_json("{oops"), None);
    }
}
