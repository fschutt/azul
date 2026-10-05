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
    // ---- the toolbar and the status bar ----
    TOOLBAR_REFRESH = "toolbar-refresh";
    TOOLBAR_ADD = "toolbar-add";
    TOOLBAR_IMPORT = "toolbar-import";
    TOOLBAR_EXPORT = "toolbar-export";
    TOOLBAR_MARK_ALL = "toolbar-mark-all";
    TOOLBAR_SETTINGS = "toolbar-settings";
    NEWS_STATUS = "news-status";
    // ---- the navigation pane ----
    NAV_ADD = "nav-add";
    // ---- the article list ----
    LIST_SEARCH = "list-search";
    LIST_FILTER = "list-filter";
    LIST_HEADING = "list-heading";
    ARTICLE_LIST = "article-list";
    /// The class of an article's row.
    ARTICLE_ROW_CLASS = "article-row";
    /// The class of a day's header in the list.
    DAY_HEADER_CLASS = "day-header";
    /// "Mark all as read?" and its answers.
    MARK_ALL_CONFIRM = "mark-all-confirm";
    MARK_ALL_YES = "mark-all-yes";
    MARK_ALL_NO = "mark-all-no";
    // ---- the reading pane ----
    READER = "reader";
    READER_OPEN = "reader-open";
    READER_STAR = "reader-star";
    READER_LATER = "reader-later";
    READER_UNREAD = "reader-unread";
    READER_PREV = "reader-prev";
    READER_NEXT = "reader-next";
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
    /// An article's row in the list (index = its place in the list).
    article = "article-";
    /// A feed found on a web page (Add feed).
    found = "found-";
    /// A row of the OPML import preview.
    opml_row = "opml-row-";
}

plain! {
    /// The article's own root in the reader document (the reader stylesheet's scope).
    ARTICLE_CLASS = "article";
    /// A picture that is not loaded: its placeholder.
    IMAGE_PLACEHOLDER_CLASS = "image-placeholder";
    /// A figure's caption.
    CAPTION_CLASS = "caption";
}
