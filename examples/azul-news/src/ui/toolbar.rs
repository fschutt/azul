//! Mail's unified toolbar (Leopard): a grey gradient bar of big tools - an icon on a bevelled
//! face, its label under it - in groups, the search field at the right end with "Search" under
//! it. Get News, Mark Read / Unread, Star, Read Later, Open (in the browser), Copy Link (to share
//! it), Mark All Read, Add Feed, Sources. A tool that has nothing to act on (no article open, an
//! article without a link) is dimmed and inert. Every tool is a keyboard stop; Enter / Space
//! activate it (`Click`), as the app's own keys do (R, M, S, L, O, Mod+N).

use azul::{
    callbacks::{CallbackType, TextInputOnTextInputCallbackType},
    dom::TabIndex,
    prelude::*,
    str::String as AzString,
};
use azul_appkit::pieces::{block, text};

use super::{
    on_add_open, on_copy_link, on_later, on_mark_all, on_open_original, on_refresh, on_search,
    on_sources_open, on_star, on_toggle_read, NewsApp, Reading,
};
use crate::ids;

/// The bar: Mail's unified grey, light and dark.
const BAR: &str = "display: flex; flex-direction: row; align-items: flex-end; \
                   padding: 4px 10px 3px 10px; gap: 2px; overflow: hidden; color: #262626; \
                   border-bottom: 1px solid #9c9c9c; \
                   background: linear-gradient(to bottom, #e8e8e8, #c6c6c6); \
                   @media (prefers-color-scheme: dark) { color: #e6e6e6; \
                   border-bottom: 1px solid #141414; \
                   background: linear-gradient(to bottom, #3e3e3e, #2b2b2b); }";
const TOOL: &str = "display: flex; flex-direction: column; align-items: center; \
                    flex-shrink: 0; min-width: 50px; padding: 2px 5px; border-radius: 4px; \
                    cursor: pointer; :hover { background: rgba(0, 0, 0, 0.07); }";
const TOOL_OFF: &str = "display: flex; flex-direction: column; align-items: center; \
                        flex-shrink: 0; min-width: 50px; padding: 2px 5px; opacity: 0.4;";
/// The tool's face: a bevelled capsule around the icon.
const FACE: &str = "display: flex; align-items: center; justify-content: center; \
                    width: 36px; height: 24px; border-radius: 6px; \
                    border: 1px solid #8e8e8e; \
                    background: linear-gradient(to bottom, #fefefe, #dcdcdc); \
                    @media (prefers-color-scheme: dark) { border: 1px solid #1e1e1e; \
                    background: linear-gradient(to bottom, #5c5c5c, #454545); }";
const ICON: &str = "font-size: 17px;";
const LABEL: &str = "padding-top: 2px; font-size: 11px; white-space: nowrap;";
/// The room between two groups of tools.
const GAP: &str = "width: 12px; flex-shrink: 0;";

/// One tool: `icon` on its face, `label` under it; `cb` with the app when it is clicked
/// (`enabled`), dimmed and inert otherwise.
fn tool(
    app: &RefAny,
    id: AzString,
    (label, icon): (&str, &str),
    cb: CallbackType,
    enabled: bool,
) -> Dom {
    let out = Dom::create_div()
        .with_id(id)
        .with_css(if enabled { TOOL } else { TOOL_OFF })
        .with_accessibility_name(label)
        .with_child(
            Dom::create_div()
                .with_css(FACE)
                .with_child(Dom::create_icon(icon).with_css(ICON)),
        )
        .with_child(block(LABEL, text(label)));
    if enabled {
        out.with_tab_index(TabIndex::Auto).with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            app.clone(),
            cb,
        )
    } else {
        out.with_tab_index(TabIndex::NoKeyboardFocus)
    }
}

fn gap() -> Dom {
    Dom::create_div().with_css(GAP)
}

/// The search field, "Search" under it (the widget's own root keeps its look: it sits in a
/// box of the toolbar's).
fn search(s: &NewsApp, app: &RefAny) -> Dom {
    let field = TextInput::create_search()
        .with_text(s.query.as_str())
        .with_placeholder("Search")
        .with_accessibility_name("Search articles")
        .with_on_text_input(app.clone(), on_search as TextInputOnTextInputCallbackType)
        .dom()
        .with_id(ids::LIST_SEARCH);
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; align-items: center; width: 200px; \
             min-width: 120px; flex-shrink: 1;",
        )
        .with_child(block("width: 100%;", field))
        .with_child(block(LABEL, text("Search")))
}

/// The toolbar.
pub(super) fn toolbar(s: &NewsApp, app: &RefAny) -> Dom {
    let lib = &s.library;
    let open = s
        .selected_ref()
        .filter(|_| matches!(s.reading, Reading::Article));
    let article = open.is_some();
    let linked = open.is_some() && s.selected_link().is_some();
    let read = open.is_some_and(|r| lib.is_read(r));
    let starred = open.is_some_and(|r| lib.is_starred(r));
    let later = open.is_some_and(|r| lib.is_later(r));
    let refresh = if s.refreshing > 0 {
        "Fetching\u{2026}"
    } else {
        "Get News"
    };
    let tools = vec![
        tool(
            app,
            ids::TOOLBAR_REFRESH,
            (refresh, "refresh"),
            on_refresh,
            true,
        ),
        gap(),
        tool(
            app,
            ids::TOOLBAR_MARK_READ,
            if read {
                ("Mark Unread", "mark_email_unread")
            } else {
                ("Mark Read", "mark_email_read")
            },
            on_toggle_read,
            article,
        ),
        tool(
            app,
            ids::TOOLBAR_STAR,
            if starred {
                ("Unstar", "star")
            } else {
                ("Star", "star_border")
            },
            on_star,
            article,
        ),
        tool(
            app,
            ids::TOOLBAR_LATER,
            if later {
                ("Not Later", "bookmark")
            } else {
                ("Read Later", "bookmark_border")
            },
            on_later,
            article,
        ),
        gap(),
        tool(
            app,
            ids::TOOLBAR_OPEN,
            ("Open", "open_in_new"),
            on_open_original,
            linked,
        ),
        tool(
            app,
            ids::TOOLBAR_SHARE,
            ("Copy Link", "link"),
            on_copy_link,
            linked,
        ),
        gap(),
        tool(
            app,
            ids::TOOLBAR_MARK_ALL,
            ("Mark All Read", "done_all"),
            on_mark_all,
            true,
        ),
        gap(),
        tool(
            app,
            ids::TOOLBAR_ADD,
            ("Add Feed", "add"),
            on_add_open,
            true,
        ),
        tool(
            app,
            ids::TOOLBAR_SOURCES,
            ("Sources", "rss_feed"),
            on_sources_open,
            true,
        ),
        Dom::create_div().with_css("flex-grow: 1; min-width: 8px;"),
        search(s, app),
    ];
    let mut bar = Dom::create_div().with_id(ids::TOOLBAR).with_css(BAR);
    for t in tools {
        bar.add_child(t);
    }
    bar
}
