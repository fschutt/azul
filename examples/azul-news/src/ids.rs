//! Every DOM id and class AzNews names, defined ONCE, each with the app's prefix `__aznews_`
//! (the widgets' own names carry `__azul_`; user ruling 2026-10-02): no clash with a widget's
//! or another app's names, and no string literal repeated. A fixed name is a `const AzString`
//! (`AzString::from_const_str` borrows the static bytes); a name made at run time (an article's
//! row, a found feed) is the prefix, its stem and the index, made by the one function here that
//! knows the stem. `scripts/aznews_e2e.py` puts the same prefix in front of the same stems.

use azul::str::String as AzString;

/// The prefix of every name.
pub const PREFIX: &str = "__aznews_";

macro_rules! names {
    ($($(#[$doc:meta])* $name:ident = $value:literal;)*) => {
        $($(#[$doc])* pub const $name: AzString = AzString::from_const_str(concat!("__aznews_", $value));)*
    };
}

/// A name per row: `<stem><index>`.
macro_rules! indexed {
    ($($(#[$doc:meta])* $name:ident = $stem:literal;)*) => {
        $($(#[$doc])* #[must_use] pub fn $name(index: usize) -> AzString {
            AzString::from(format!(concat!("__aznews_", $stem, "{}"), index))
        })*
    };
}

/// The plain-text form of a name, for the reader's own document (its classes are written as
/// text into the parsed tree): `__aznews_<stem>`.
macro_rules! plain {
    ($($(#[$doc:meta])* $name:ident = $value:literal;)*) => {
        $($(#[$doc])* pub const $name: &str = concat!("__aznews_", $value);)*
    };
}

names! {
    // ---- the toolbar (Mail's: big icons, their labels under them, the search at the right) ----
    TOOLBAR = "toolbar";
    TOOLBAR_REFRESH = "toolbar-refresh";
    TOOLBAR_MARK_READ = "toolbar-mark-read";
    TOOLBAR_STAR = "toolbar-star";
    TOOLBAR_LATER = "toolbar-later";
    TOOLBAR_OPEN = "toolbar-open";
    TOOLBAR_SHARE = "toolbar-share";
    TOOLBAR_MARK_ALL = "toolbar-mark-all";
    TOOLBAR_ADD = "toolbar-add";
    TOOLBAR_SOURCES = "toolbar-sources";
    // ---- the source list ----
    SIDEBAR = "sidebar";
    /// The "Articles" section and its rows.
    SECTION_ARTICLES = "section-articles";
    VIEW_ALL = "view-all";
    VIEW_UNREAD = "view-unread";
    VIEW_STARRED = "view-starred";
    VIEW_LATER = "view-later";
    VIEW_BROKEN = "view-broken";
    /// The activity area at the bottom of the source list (refreshing, n of m feeds).
    ACTIVITY = "activity";
    /// The buttons under the source list: add a feed, show the activity, the actions menu.
    NAV_ADD = "nav-add";
    NAV_ACTIVITY = "nav-activity";
    NAV_ACTIONS = "nav-actions";
    /// The class of a source's section header.
    SECTION_CLASS = "section";
    /// The class of a row of the source list.
    NAV_ROW_CLASS = "nav-row";
    // ---- the article table ----
    LIST_SEARCH = "list-search";
    LIST_FILTER = "list-filter";
    LIST_HEADING = "list-heading";
    /// The table's rows (a VirtualView: only the rows in view are built).
    ARTICLE_LIST = "article-list";
    /// The column headers (a click sorts, again turns round).
    TABLE_HEADER = "table-header";
    SORT_UNREAD = "sort-unread";
    SORT_STARRED = "sort-starred";
    SORT_TITLE = "sort-title";
    SORT_SOURCE = "sort-source";
    SORT_DATE = "sort-date";
    /// The class of an article's row.
    ARTICLE_ROW_CLASS = "article-row";
    /// The class of a day's header in the table.
    DAY_HEADER_CLASS = "day-header";
    /// "Mark all as read?" and its answers.
    MARK_ALL_CONFIRM = "mark-all-confirm";
    MARK_ALL_YES = "mark-all-yes";
    MARK_ALL_NO = "mark-all-no";
    // ---- the reading pane ----
    READER = "reader";
    // ---- add a feed ----
    ADD_FEED = "add-feed";
    ADD_URL = "add-url";
    ADD_FIND = "add-find";
    ADD_FOLDER = "add-folder";
    ADD_NEW_FOLDER = "add-new-folder";
    ADD_PROBLEM = "add-problem";
    ADD_CANCEL = "add-cancel";
    ADD_SUBSCRIBE = "add-subscribe";
    // ---- OPML import ----
    OPML_IMPORT = "opml-import";
    OPML_PATH = "opml-path";
    OPML_READ = "opml-read";
    OPML_CHOOSE = "opml-choose";
    OPML_SUMMARY = "opml-summary";
    OPML_ROWS = "opml-rows";
    OPML_CANCEL = "opml-cancel";
    OPML_RUN = "opml-run";
    // ---- a feed's own page (rename, folder, unsubscribe) ----
    FEED_PAGE = "feed-page";
    FEED_TITLE = "feed-title";
    FEED_FOLDER = "feed-folder";
    FEED_REFRESH = "feed-refresh";
    FEED_UNSUBSCRIBE = "feed-unsubscribe";
    FEED_UNSUBSCRIBE_CONFIRM = "feed-unsubscribe-confirm";
    FEED_DONE = "feed-done";
    // ---- the sources page (which feeds are followed; add, import, export, remove) ----
    SOURCES = "sources";
    SOURCES_ADD = "sources-add";
    SOURCES_IMPORT = "sources-import";
    SOURCES_EXPORT = "sources-export";
    SOURCES_REFRESH = "sources-refresh";
    SOURCES_DONE = "sources-done";
    // ---- the settings ----
    SET_FONT_SIZE = "set-font-size";
    SET_LINE_WIDTH = "set-line-width";
    SET_PAPER = "set-paper";
    SET_IMAGES = "set-images";
    SET_REFRESH_START = "set-refresh-start";
    SET_REFRESH_EVERY = "set-refresh-every";
    SET_STRIP_TRACKING = "set-strip-tracking";
    SET_KEEP_DAYS = "set-keep-days";
}

indexed! {
    /// An article's row in the table (index = its place among the table's articles).
    article = "article-";
    /// A feed found on a web page (Add feed).
    found = "found-";
    /// A row of the OPML import preview.
    opml_row = "opml-row-";
    /// A source's section header in the source list (index = the feed's place in the list).
    section = "section-";
    /// A source's "All" row in the source list.
    source_all = "source-all-";
    /// A source's row on the sources page, and its controls.
    source_row = "source-row-";
    source_follow = "source-follow-";
    source_edit = "source-edit-";
    source_remove = "source-remove-";
    source_remove_yes = "source-remove-yes-";
    source_remove_no = "source-remove-no-";
}

/// A topic's row in the source list: `topic-<feed>-<topic>` (the feed's place in the list, the
/// topic's among the feed's topics).
#[must_use]
pub fn topic(feed: usize, topic: usize) -> AzString {
    AzString::from(format!("{PREFIX}topic-{feed}-{topic}"))
}

plain! {
    /// The article's own root in the reader document (the reader stylesheet's scope).
    ARTICLE_CLASS = "article";
    /// A picture that is not loaded: its placeholder.
    IMAGE_PLACEHOLDER_CLASS = "image-placeholder";
    /// A figure's caption.
    CAPTION_CLASS = "caption";
}
