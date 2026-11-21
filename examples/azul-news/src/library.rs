//! The subscriptions, their articles and their marks - what the window shows, as plain data.
//!
//! - the views of the source list ([`View`]): all articles, unread, starred, read later, a
//!   folder, one feed, one topic of a feed (its own categories: a forum's subforum, a blog's
//!   category - [`Library::topics`]), the feeds that failed;
//! - the list: the view's articles newest first, filtered by the search box (every word, in any
//!   case, diacritics folded: azul-pim's one search), in Outlook's date groups (azul-pim's
//!   [`DateGroup`]), or in the order of the table's column the user clicked ([`Sort`]);
//! - the unread counts the source list shows; a feed the user stopped following
//!   ([`Subscription::paused`]) is kept but left out of "All articles", "Unread" and their counts;
//! - a refresh merged in ([`Library::merge`]): an article keeps the time AzNews first saw it; an
//!   article the feed dropped stays while it is inside the "keep" window, and always when it is
//!   starred or kept for later; at most [`MAX_ITEMS`] per feed; the read marks of articles gone
//!   are forgotten.

use std::collections::{BTreeMap, BTreeSet};

use azul_pim::{
    dates::{date_group, DateGroup},
    search::{fold, Query},
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
/// The most topics of one feed the source list shows (the most common ones).
pub const MAX_TOPICS: usize = 12;

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
    /// The articles of one feed that carry one of its topics: (feed id, topic).
    Topic(String, String),
    /// The feeds whose last refresh failed.
    Broken,
}

/// One topic of a feed - an RSS `<category>`, an Atom `<category term>`, a JSON Feed tag: a
/// forum's subforum, a blog's category - and how many of the feed's articles carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Topic {
    pub name: String,
    pub articles: usize,
    pub unread: usize,
}

/// What the article table is sorted by (a click on a column's header).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Date,
    Title,
    /// The feed's name.
    Source,
    /// Read or unread.
    Unread,
    Starred,
}

impl SortKey {
    pub const ALL: [SortKey; 5] = [
        SortKey::Date,
        SortKey::Title,
        SortKey::Source,
        SortKey::Unread,
        SortKey::Starred,
    ];

    /// Its name in the settings.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            SortKey::Date => "date",
            SortKey::Title => "title",
            SortKey::Source => "source",
            SortKey::Unread => "unread",
            SortKey::Starred => "starred",
        }
    }

    /// The key a settings value names (the date for anything else).
    #[must_use]
    pub fn parse(key: Option<&str>) -> SortKey {
        SortKey::ALL
            .into_iter()
            .find(|k| Some(k.key()) == key)
            .unwrap_or_default()
    }

    /// The way a first click on its column sorts: newest, unread and starred first; titles and
    /// sources A to Z.
    #[must_use]
    pub fn first_descending(self) -> bool {
        matches!(self, SortKey::Date | SortKey::Unread | SortKey::Starred)
    }
}

/// The table's order: a column and its direction (`descending`: newest, Z, unread, starred
/// first).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sort {
    pub key: SortKey,
    pub descending: bool,
}

impl Default for Sort {
    fn default() -> Self {
        Sort {
            key: SortKey::Date,
            descending: true,
        }
    }
}

impl Sort {
    /// The order after a click on `key`'s header: the same column turns round, another one
    /// starts the way it reads best ([`SortKey::first_descending`]).
    #[must_use]
    pub fn clicked(self, key: SortKey) -> Sort {
        if self.key == key {
            Sort {
                key,
                descending: !self.descending,
            }
        } else {
            Sort {
                key,
                descending: key.first_descending(),
            }
        }
    }
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
    pub fn feed_index(&self, id: &str) -> Option<usize> {
        self.feeds.iter().position(|f| f.sub.id == id)
    }

    /// The folders, in the order their first feed comes.
    #[must_use]
    pub fn folders(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for f in &self.feeds {
            if !f.sub.folder.is_empty() && !out.contains(&f.sub.folder) {
                out.push(f.sub.folder.clone());
            }
        }
        out
    }

    /// The unread articles of one feed.
    #[must_use]
    pub fn unread(&self, feed: usize) -> usize {
        self.feeds.get(feed).map_or(0, |f| {
            f.items.iter().filter(|i| !f.state.is_read(&i.id)).count()
        })
    }

    /// The unread articles of every feed that is followed ("All articles").
    #[must_use]
    pub fn unread_total(&self) -> usize {
        self.followed().into_iter().map(|f| self.unread(f)).sum()
    }

    /// The feeds that are followed: refreshed, in "All articles" (in the list's order).
    #[must_use]
    pub fn followed(&self) -> Vec<usize> {
        (0..self.feeds.len())
            .filter(|&f| !self.feeds[f].sub.paused)
            .collect()
    }

    /// Follows a feed again or stops following it; whether that changed anything.
    pub fn set_paused(&mut self, feed: usize, paused: bool) -> bool {
        match self.feeds.get_mut(feed) {
            Some(f) if f.sub.paused != paused => {
                f.sub.paused = paused;
                true
            }
            _ => false,
        }
    }

    /// A feed's topics: the categories its articles carry, the most common first (the same
    /// count A to Z), at most [`MAX_TOPICS`]; none when no article carries one.
    #[must_use]
    pub fn topics(&self, feed: usize) -> Vec<Topic> {
        let Some(f) = self.feeds.get(feed) else {
            return Vec::new();
        };
        let mut counts: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for item in &f.items {
            let unread = !f.state.is_read(&item.id);
            let mut seen: BTreeSet<&str> = BTreeSet::new();
            for name in item.categories.iter().map(|c| c.trim()) {
                if name.is_empty() || !seen.insert(name) {
                    continue;
                }
                let count = counts.entry(name.to_string()).or_insert((0, 0));
                count.0 += 1;
                if unread {
                    count.1 += 1;
                }
            }
        }
        let mut out: Vec<Topic> = counts
            .into_iter()
            .map(|(name, (articles, unread))| Topic {
                name,
                articles,
                unread,
            })
            .collect();
        out.sort_by(|a, b| {
            b.articles
                .cmp(&a.articles)
                .then_with(|| fold(&a.name).cmp(&fold(&b.name)))
        });
        out.truncate(MAX_TOPICS);
        out
    }

    #[must_use]
    pub fn unread_in_folder(&self, folder: &str) -> usize {
        (0..self.feeds.len())
            .filter(|&f| self.feeds[f].sub.folder == folder)
            .map(|f| self.unread(f))
            .sum()
    }

    #[must_use]
    pub fn starred_count(&self) -> usize {
        self.feeds
            .iter()
            .map(|f| f.items.iter().filter(|i| f.state.is_starred(&i.id)).count())
            .sum()
    }

    #[must_use]
    pub fn later_count(&self) -> usize {
        self.feeds
            .iter()
            .map(|f| f.items.iter().filter(|i| f.state.is_later(&i.id)).count())
            .sum()
    }

    /// The feeds whose last refresh failed.
    #[must_use]
    pub fn broken(&self) -> Vec<usize> {
        (0..self.feeds.len())
            .filter(|&f| !self.feeds[f].meta.error.is_empty())
            .collect()
    }

    #[must_use]
    pub fn article(&self, r: ArticleRef) -> Option<&Item> {
        self.feeds.get(r.feed)?.items.get(r.item)
    }

    /// Whether the article is in the view.
    fn in_view(&self, view: &View, r: ArticleRef) -> bool {
        let f = &self.feeds[r.feed];
        let item = &f.items[r.item];
        let id = item.id.as_str();
        match view {
            View::All => !f.sub.paused,
            View::Unread => !f.sub.paused && !f.state.is_read(id),
            View::Starred => f.state.is_starred(id),
            View::Later => f.state.is_later(id),
            View::Folder(folder) => f.sub.folder == *folder,
            View::Feed(feed) => f.sub.id == *feed,
            View::Topic(feed, topic) => {
                f.sub.id == *feed && item.categories.iter().any(|c| c.trim() == topic.as_str())
            }
            View::Broken => !f.meta.error.is_empty(),
        }
    }

    /// `refs` in the order `sort` asks for; within the same value newest first, then in the
    /// list's own order.
    pub fn sort(&self, refs: &mut Vec<ArticleRef>, sort: Sort) {
        let mut keyed: Vec<((i64, String), i64, ArticleRef)> = refs
            .iter()
            .map(|&r| {
                let date = self.article(r).map_or(0, Item::date);
                let value = match sort.key {
                    SortKey::Date => (date, String::new()),
                    SortKey::Title => (
                        0,
                        self.article(r)
                            .map_or_else(String::new, |i| fold(i.title.trim())),
                    ),
                    SortKey::Source => (
                        0,
                        self.feeds
                            .get(r.feed)
                            .map_or_else(String::new, |f| fold(f.name())),
                    ),
                    SortKey::Unread => (i64::from(!self.is_read(r)), String::new()),
                    SortKey::Starred => (i64::from(self.is_starred(r)), String::new()),
                };
                (value, date, r)
            })
            .collect();
        keyed.sort_by(|a, b| {
            let primary = if sort.descending {
                b.0.cmp(&a.0)
            } else {
                a.0.cmp(&b.0)
            };
            primary
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        *refs = keyed.into_iter().map(|(_, _, r)| r).collect();
    }

    /// The view's articles that match `query`, newest first.
    #[must_use]
    pub fn list(&self, view: &View, query: &str) -> Vec<ArticleRef> {
        let query = Query::parse(query);
        let mut out: Vec<(i64, ArticleRef)> = Vec::new();
        for (fi, f) in self.feeds.iter().enumerate() {
            for (ii, item) in f.items.iter().enumerate() {
                let r = ArticleRef { feed: fi, item: ii };
                if !self.in_view(view, r) {
                    continue;
                }
                if !query.is_empty() {
                    let text = format!(
                        "{} {} {} {}",
                        item.title,
                        item.excerpt,
                        item.author,
                        f.name()
                    );
                    if !query.matches(&text) {
                        continue;
                    }
                }
                out.push((item.date(), r));
            }
        }
        // Newest first; the same date in the feeds' and the items' order.
        out.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        out.into_iter().map(|(_, r)| r).collect()
    }

    /// `refs` (newest first) in their date groups, `today` and the dates in the local time
    /// `offset_secs` east of UTC.
    #[must_use]
    pub fn sections(
        &self,
        refs: &[ArticleRef],
        today: NaiveDate,
        offset_secs: i64,
    ) -> Vec<(DateGroup, Vec<ArticleRef>)> {
        let mut out: Vec<(DateGroup, Vec<ArticleRef>)> = Vec::new();
        for &r in refs {
            let Some(item) = self.article(r) else {
                continue;
            };
            let day = chrono::DateTime::<chrono::Utc>::from_timestamp(item.date() + offset_secs, 0)
                .map_or(NaiveDate::MIN, |d| d.date_naive());
            let group = date_group(day, today);
            match out.last_mut() {
                Some((g, members)) if *g == group => members.push(r),
                _ => out.push((group, vec![r])),
            }
        }
        out
    }

    fn id_of(&self, r: ArticleRef) -> Option<String> {
        self.article(r).map(|i| i.id.clone())
    }

    #[must_use]
    pub fn is_read(&self, r: ArticleRef) -> bool {
        self.id_of(r)
            .is_some_and(|id| self.feeds[r.feed].state.is_read(&id))
    }

    /// Marks an article read or unread; whether that changed anything.
    pub fn set_read(&mut self, r: ArticleRef, read: bool) -> bool {
        match self.id_of(r) {
            Some(id) => self.feeds[r.feed].state.set_read(&id, read),
            None => false,
        }
    }

    #[must_use]
    pub fn is_starred(&self, r: ArticleRef) -> bool {
        self.id_of(r)
            .is_some_and(|id| self.feeds[r.feed].state.is_starred(&id))
    }

    /// Stars or unstars an article; its new state.
    pub fn toggle_star(&mut self, r: ArticleRef) -> bool {
        match self.id_of(r) {
            Some(id) => self.feeds[r.feed].state.toggle_starred(&id),
            None => false,
        }
    }

    #[must_use]
    pub fn is_later(&self, r: ArticleRef) -> bool {
        self.id_of(r)
            .is_some_and(|id| self.feeds[r.feed].state.is_later(&id))
    }

    /// Keeps an article for later, or not; its new state.
    pub fn toggle_later(&mut self, r: ArticleRef) -> bool {
        match self.id_of(r) {
            Some(id) => self.feeds[r.feed].state.toggle_later(&id),
            None => false,
        }
    }

    /// Marks the articles read; the feeds whose marks changed (their files to write), in order.
    pub fn mark_all_read(&mut self, refs: &[ArticleRef]) -> Vec<usize> {
        let mut changed = BTreeSet::new();
        for &r in refs {
            if self.set_read(r, true) {
                changed.insert(r.feed);
            }
        }
        changed.into_iter().collect()
    }

    /// A refresh of feed `feed` merged in (see the module documentation); how many articles
    /// are new.
    pub fn merge(&mut self, feed: usize, parsed: Feed, now: i64, keep_days: u32) -> usize {
        let Some(data) = self.feeds.get_mut(feed) else {
            return 0;
        };
        let old: BTreeMap<String, Item> = data.items.drain(..).map(|i| (i.id.clone(), i)).collect();
        let mut fresh = 0;
        let mut items: Vec<Item> = Vec::new();
        let mut present: BTreeSet<String> = BTreeSet::new();
        for mut item in parsed.items {
            if !present.insert(item.id.clone()) {
                continue;
            }
            match old.get(&item.id) {
                Some(before) => item.seen = before.seen,
                None => {
                    item.seen = now;
                    fresh += 1;
                }
            }
            items.push(item);
        }
        let keep_after = now - i64::from(keep_days) * DAY;
        for (id, item) in old {
            if present.contains(&id) {
                continue;
            }
            let marked = data.state.is_starred(&id) || data.state.is_later(&id);
            if marked || item.date() >= keep_after {
                present.insert(id);
                items.push(item);
            }
        }
        items.sort_by(|a, b| b.date().cmp(&a.date()));
        // At most MAX_ITEMS, and every starred / saved one.
        let mut kept = 0;
        let state = &data.state;
        items.retain(|i| {
            if state.is_starred(&i.id) || state.is_later(&i.id) {
                return true;
            }
            kept += 1;
            kept <= MAX_ITEMS
        });
        let ids: BTreeSet<&str> = items.iter().map(|i| i.id.as_str()).collect();
        data.state.prune(&ids);
        data.items = items;
        data.meta.title = parsed.title;
        if !parsed.site.is_empty() {
            data.meta.site = parsed.site;
        }
        if !parsed.icon.is_empty() {
            data.meta.icon = parsed.icon;
        }
        data.meta.kind = parsed.format.label().to_string();
        data.meta.error.clear();
        data.meta.checked = now;
        if fresh > 0 {
            data.meta.updated = now;
        }
        fresh
    }

    /// Adds a feed (at the end); its index. A feed whose address is subscribed already is not
    /// added twice: that one's index.
    pub fn subscribe(&mut self, sub: Subscription) -> usize {
        if let Some(i) = self.feeds.iter().position(|f| f.sub.url == sub.url) {
            return i;
        }
        self.feeds.push(FeedData {
            meta: FeedMeta {
                title: sub.title.clone(),
                site: sub.site.clone(),
                ..FeedMeta::default()
            },
            sub,
            items: Vec::new(),
            state: ReadState::default(),
        });
        self.feeds.len() - 1
    }

    /// Removes the feed with this id; what it was.
    pub fn unsubscribe(&mut self, id: &str) -> Option<FeedData> {
        let i = self.feed_index(id)?;
        Some(self.feeds.remove(i))
    }

    /// The subscription list (for `subscriptions.opml`).
    #[must_use]
    pub fn subscriptions(&self) -> Vec<Subscription> {
        self.feeds.iter().map(|f| f.sub.clone()).collect()
    }

    /// The article after (or before) `current` in `list`; the first (last) one without one.
    #[must_use]
    pub fn next(
        &self,
        list: &[ArticleRef],
        current: Option<ArticleRef>,
        forward: bool,
    ) -> Option<ArticleRef> {
        let at = current.and_then(|c| list.iter().position(|r| *r == c));
        match (at, forward) {
            (None, true) => list.first().copied(),
            (None, false) => list.last().copied(),
            (Some(i), true) => list.get(i + 1).copied(),
            (Some(i), false) => i.checked_sub(1).and_then(|j| list.get(j).copied()),
        }
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
                paused: false,
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
                    vec![
                        item("a1", "The quiet return of RSS", NOW - 3_600),
                        item("a2", "Older essay", NOW - 3 * DAY),
                    ],
                ),
                feed_data(
                    "b",
                    "Rust Blog",
                    "Tech",
                    vec![item("b1", "Rust 2026 survey results", NOW - 2 * 3_600)],
                ),
                feed_data(
                    "c",
                    "Bakery News",
                    "Local",
                    vec![item("c1", "New bakery opens on Main St.", NOW - DAY - 60)],
                ),
            ],
        }
    }

    fn r(feed: usize, item: usize) -> ArticleRef {
        ArticleRef { feed, item }
    }

    #[test]
    fn the_list_is_newest_first_across_feeds() {
        let lib = library();
        assert_eq!(
            lib.list(&View::All, ""),
            vec![r(0, 0), r(1, 0), r(2, 0), r(0, 1)]
        );
        assert_eq!(
            lib.list(&View::Folder("Tech".into()), ""),
            vec![r(0, 0), r(1, 0), r(0, 1)]
        );
        assert_eq!(lib.list(&View::Feed("c".into()), ""), vec![r(2, 0)]);
        assert_eq!(lib.folders(), vec!["Tech".to_string(), "Local".to_string()]);
        assert_eq!(lib.feed_index("b"), Some(1));
        assert_eq!(lib.feed_index("zzz"), None);
    }

    #[test]
    fn the_search_box_finds_every_word_in_any_case() {
        let lib = library();
        assert_eq!(lib.list(&View::All, "rss QUIET"), vec![r(0, 0)]);
        assert_eq!(
            lib.list(&View::All, "rust blog"),
            vec![r(1, 0)],
            "the feed's name counts too"
        );
        assert_eq!(
            lib.list(&View::All, "nothing like this"),
            Vec::<ArticleRef>::new()
        );
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
        assert_eq!(
            lib.mark_all_read(&[r(1, 0), r(0, 1), r(0, 0)]),
            vec![0, 1],
            "the feeds that changed"
        );
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
        assert_eq!(
            groups,
            vec![
                DateGroup::Today,
                DateGroup::Yesterday,
                DateGroup::Weekday(chrono::Weekday::Mon)
            ]
        );
        assert_eq!(sections[0].1, vec![r(0, 0), r(1, 0)]);
        // 13 hours west of UTC the article of 11:00 UTC is still today, the one of yesterday
        // 11:59 UTC too is yesterday.
        let west = lib.sections(
            &list,
            NaiveDate::from_ymd_opt(2026, 9, 30).expect("a date"),
            -13 * 3_600,
        );
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
        assert_eq!(
            ids,
            vec!["a3", "a1", "a2"],
            "newest first; the dropped article stays (starred, and inside 30 days)"
        );
        assert_eq!(
            lib.feeds[0].items[1].title,
            "The quiet return of RSS (edited)"
        );
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
        assert_eq!(
            lib.merge(
                0,
                Feed {
                    items: vec![item("a1", "x", NOW - 3_600)],
                    ..Feed::default()
                },
                NOW,
                30
            ),
            0
        );
        let ids: Vec<&str> = lib.feeds[0].items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, vec!["a1"]);
        assert!(
            !lib.feeds[0].state.is_read("a2"),
            "its read mark is forgotten"
        );
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
        assert_eq!(
            lib.subscribe(Subscription {
                id: "e".into(),
                ..sub
            }),
            3
        );
        assert_eq!(lib.feeds.len(), 4);
        assert_eq!(lib.subscriptions().len(), 4);
        assert_eq!(
            lib.unsubscribe("b").map(|f| f.sub.title),
            Some("Rust Blog".to_string())
        );
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
    fn a_feeds_topics_are_its_categories_most_common_first_with_their_unread_counts() {
        let mut lib = library();
        // Example Weekly: a1 "Essays" and "RSS" (twice, once with blanks), a2 "Essays".
        lib.feeds[0].items[0].categories = vec![
            "Essays".to_string(),
            "RSS".to_string(),
            " RSS ".to_string(),
            String::new(),
        ];
        lib.feeds[0].items[1].categories = vec!["Essays".to_string()];
        lib.set_read(r(0, 1), true);
        assert_eq!(
            lib.topics(0),
            vec![
                Topic {
                    name: "Essays".to_string(),
                    articles: 2,
                    unread: 1,
                },
                Topic {
                    name: "RSS".to_string(),
                    articles: 1,
                    unread: 1,
                },
            ],
            "an article counts once per topic; a blank is no topic"
        );
        assert_eq!(lib.topics(1), Vec::<Topic>::new(), "no categories: no topics");
        assert_eq!(lib.topics(99), Vec::<Topic>::new());
        assert_eq!(
            lib.list(&View::Topic("a".into(), "RSS".into()), ""),
            vec![r(0, 0)]
        );
        assert_eq!(
            lib.list(&View::Topic("a".into(), "Essays".into()), ""),
            vec![r(0, 0), r(0, 1)]
        );
        assert_eq!(
            lib.list(&View::Topic("b".into(), "Essays".into()), ""),
            Vec::<ArticleRef>::new(),
            "another feed's topic of the same name is not this one"
        );
    }

    #[test]
    fn a_feed_shows_at_most_its_most_common_topics() {
        let mut lib = library();
        lib.feeds[1].items = (0..20)
            .map(|n| {
                let mut i = item(&format!("t{n}"), "x", NOW - n * 60);
                // "z" on every article, "k<n>" on one each.
                i.categories = vec!["z".to_string(), format!("k{n:02}")];
                i
            })
            .collect();
        let topics = lib.topics(1);
        assert_eq!(topics.len(), MAX_TOPICS);
        assert_eq!(topics[0].name, "z");
        assert_eq!(topics[0].articles, 20);
        assert_eq!(topics[1].name, "k00", "the same count A to Z");
    }

    #[test]
    fn a_feed_not_followed_is_kept_but_left_out_of_all_and_unread() {
        let mut lib = library();
        assert!(lib.set_paused(2, true));
        assert!(!lib.set_paused(2, true), "no change");
        assert_eq!(lib.followed(), vec![0, 1]);
        assert_eq!(lib.list(&View::All, ""), vec![r(0, 0), r(1, 0), r(0, 1)]);
        assert_eq!(
            lib.list(&View::Unread, ""),
            vec![r(0, 0), r(1, 0), r(0, 1)]
        );
        assert_eq!(lib.unread_total(), 3);
        assert_eq!(
            lib.list(&View::Feed("c".into()), ""),
            vec![r(2, 0)],
            "its own view still shows it"
        );
        assert!(lib.set_paused(2, false));
        assert_eq!(lib.unread_total(), 4);
    }

    #[test]
    fn the_table_sorts_by_the_column_clicked() {
        let mut lib = library();
        let list = lib.list(&View::All, "");
        let sorted = |lib: &Library, sort: Sort| {
            let mut refs = list.clone();
            lib.sort(&mut refs, sort);
            refs
        };
        assert_eq!(
            sorted(&lib, Sort::default()),
            list,
            "newest first is the list's own order"
        );
        let oldest = Sort {
            key: SortKey::Date,
            descending: false,
        };
        assert_eq!(
            sorted(&lib, oldest),
            vec![r(0, 1), r(2, 0), r(1, 0), r(0, 0)]
        );
        let titles = Sort::default().clicked(SortKey::Title);
        assert_eq!(
            titles,
            Sort {
                key: SortKey::Title,
                descending: false,
            }
        );
        // New bakery ..., Older essay, Rust 2026 ..., The quiet return ...
        assert_eq!(
            sorted(&lib, titles),
            vec![r(2, 0), r(0, 1), r(1, 0), r(0, 0)]
        );
        assert_eq!(
            sorted(&lib, titles.clicked(SortKey::Title)),
            vec![r(0, 0), r(1, 0), r(0, 1), r(2, 0)],
            "a second click turns it round"
        );
        // Bakery News, Example Weekly (newest first inside), Rust Blog.
        assert_eq!(
            sorted(&lib, Sort::default().clicked(SortKey::Source)),
            vec![r(2, 0), r(0, 0), r(0, 1), r(1, 0)]
        );
        lib.set_read(r(0, 0), true);
        lib.set_read(r(2, 0), true);
        let unread = Sort::default().clicked(SortKey::Unread);
        assert!(unread.descending, "unread first");
        assert_eq!(
            sorted(&lib, unread),
            vec![r(1, 0), r(0, 1), r(0, 0), r(2, 0)]
        );
        lib.toggle_star(r(0, 1));
        assert_eq!(
            sorted(&lib, Sort::default().clicked(SortKey::Starred))[0],
            r(0, 1),
            "starred first"
        );
    }

    #[test]
    fn the_sort_key_round_trips_through_its_name() {
        for key in SortKey::ALL {
            assert_eq!(SortKey::parse(Some(key.key())), key);
        }
        assert_eq!(SortKey::parse(Some("banana")), SortKey::Date);
        assert_eq!(SortKey::parse(None), SortKey::Date);
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
        let meta = FeedMeta {
            etag: "\"abc\"".into(),
            checked: NOW,
            status: 200,
            ..FeedMeta::default()
        };
        let text = meta.to_json();
        assert!(text.contains("aznews.feed"));
        let back = FeedMeta::from_json(&text).expect("its own file");
        assert_eq!(back.etag, "\"abc\"");
        assert_eq!(back.checked, NOW);
        assert_eq!(FeedMeta::from_json("{oops"), None);
    }
}
