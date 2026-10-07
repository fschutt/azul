//! The source list - Mail's (Leopard) left column on its blue-grey ground:
//!
//! - section headers in small capitals with a disclosure triangle: ARTICLES (all, unread,
//!   starred, read later, broken feeds), then one section per SOURCE (a click on the header opens
//!   or closes it; its context menu refreshes it, marks it read, edits it, stops or starts
//!   following it, removes it);
//! - under a source: "All" and its TOPICS (the feed's own categories - a forum's subforums, a
//!   blog's categories; the most common first), each with its unread count as a pill; the row
//!   shown in the table is selected;
//! - the ACTIVITY area at the bottom ("NEWS ACTIVITY": fetching, n of m feeds, when the feeds
//!   were last asked, the last word), and under it the + (add a feed), activity and actions
//!   buttons.

use azul::{
    callbacks::CallbackType,
    menu::{Menu, MenuItem, StringMenuItem},
    prelude::*,
    str::String as AzString,
};
use azul_appkit::pieces::{block, column, text};

use super::{
    mark_read, now_secs, on_add_open, on_export, on_import_open, on_open_settings, on_refresh,
    on_sources_open, set_followed, set_view, start_refresh, updated_line, with_app, NewsApp,
    Reading,
};
use crate::{ids, library::View};

/// The column: Mail's source-list ground, light and dark.
const SIDEBAR: &str = "display: flex; flex-direction: column; flex-grow: 1; width: 100%; \
                       height: 100%; min-width: 0px; min-height: 0px; overflow: hidden; \
                       background: #dde3ea; color: #1d2633; \
                       @media (prefers-color-scheme: dark) { background: #25282d; \
                       color: #d8dde4; }";
const LIST: &str = "flex-grow: 1; min-height: 0px; overflow-y: auto; padding-bottom: 8px;";
const SECTION_HEAD: &str = "display: flex; flex-direction: row; align-items: center; \
                            height: 20px; padding: 6px 8px 0px 6px; font-size: 11px; \
                            font-weight: 700; color: #6b7889; cursor: pointer; \
                            @media (prefers-color-scheme: dark) { color: #8f9aa8; }";
const TRIANGLE: &str = "width: 13px; flex-shrink: 0; font-size: 8px;";
const CLIP: &str = "flex-grow: 1; min-width: 0px; overflow: hidden; white-space: nowrap; \
                    text-overflow: ellipsis;";
const ROW: &str = "display: flex; flex-direction: row; align-items: center; height: 20px; \
                   padding-right: 8px; font-size: 12px; cursor: pointer;";
const ROW_SELECTED: &str = "background: system:selection-background; \
                            color: system:selection-text; font-weight: 700;";
const ROW_ICON: &str = "font-size: 15px; width: 18px; flex-shrink: 0; margin-right: 5px; \
                        color: #5d6f86; \
                        @media (prefers-color-scheme: dark) { color: #9fb0c4; }";
const ROW_ICON_SELECTED: &str = "font-size: 15px; width: 18px; flex-shrink: 0; \
                                 margin-right: 5px;";
/// Mail's unread count: a grey-blue pill, white on the selected row.
const PILL: &str = "flex-shrink: 0; margin-left: 4px; padding: 0px 6px; border-radius: 8px; \
                    font-size: 10px; font-weight: 700; color: #ffffff; background: #8a9bb3; \
                    @media (prefers-color-scheme: dark) { background: #56657a; }";
const PILL_SELECTED: &str = "flex-shrink: 0; margin-left: 4px; padding: 0px 6px; \
                             border-radius: 8px; font-size: 10px; font-weight: 700; \
                             color: #2f6fd6; background: #ffffff;";
const ACTIVITY: &str = "flex-shrink: 0; padding: 4px 0px 6px 0px; font-size: 11px; \
                        border-top: 1px solid rgba(0, 0, 0, 0.18);";
const ACTIVITY_HEAD: &str = "padding: 0px 0px 4px 0px; font-size: 10px; font-weight: 700; \
                             text-align: center; opacity: 0.6;";
const ACTIVITY_LINE: &str = "padding: 1px 10px; overflow: hidden; white-space: nowrap; \
                             text-overflow: ellipsis;";
const BOTTOM: &str = "display: flex; flex-direction: row; align-items: center; \
                      flex-shrink: 0; height: 23px; \
                      border-top: 1px solid rgba(0, 0, 0, 0.25); \
                      background: linear-gradient(to bottom, #f4f4f4, #d9d9d9); \
                      @media (prefers-color-scheme: dark) { \
                      background: linear-gradient(to bottom, #3a3a3a, #2d2d2d); }";
const SMALL_BUTTON: &str = "display: flex; align-items: center; justify-content: center; \
                            width: 30px; height: 23px; font-size: 15px; cursor: pointer; \
                            border-right: 1px solid rgba(0, 0, 0, 0.22); \
                            :hover { background: rgba(0, 0, 0, 0.08); }";

/// The source list, the activity area, the buttons under them.
pub(super) fn sidebar(s: &NewsApp, app: &RefAny) -> Dom {
    let mut list = Dom::create_div()
        .with_id(ids::SIDEBAR)
        .with_css(LIST)
        .with_child(articles_section(s, app));
    for i in 0..s.library.feeds.len() {
        list.add_child(source_section(s, app, i));
    }
    let mut children = vec![list];
    if s.show_activity {
        children.push(activity(s));
    }
    children.push(bottom_bar(s, app));
    column(SIDEBAR, children)
}

/// A pill's text: the count, nothing for none.
fn pill(n: usize, selected: bool) -> Option<Dom> {
    (n > 0).then(|| {
        block(
            if selected { PILL_SELECTED } else { PILL },
            text(n.to_string()),
        )
    })
}

/// What a row of the list carries: the app and the view it shows.
struct NavRef {
    app: RefAny,
    view: View,
}

/// A row: its icon, its name, its unread pill; a click shows `view` in the table.
fn nav_row(
    s: &NewsApp,
    app: &RefAny,
    (icon, label, count): (&str, &str, usize),
    view: View,
    id: AzString,
    indent_px: u32,
) -> Dom {
    let selected = matches!(s.reading, Reading::Article) && s.view == view;
    let mut row = Dom::create_div()
        .with_id(id)
        .with_class(ids::NAV_ROW_CLASS)
        .with_css(format!(
            "{ROW} padding-left: {indent_px}px; {}",
            if selected { ROW_SELECTED } else { "" }
        ))
        .with_accessibility_name(label)
        .with_child(Dom::create_icon(icon).with_css(if selected {
            ROW_ICON_SELECTED
        } else {
            ROW_ICON
        }))
        .with_child(block(CLIP, text(label)));
    if let Some(badge) = pill(count, selected) {
        row.add_child(badge);
    }
    row.with_callback(
        EventFilter::Hover(HoverEventFilter::Click),
        RefAny::new(NavRef {
            app: app.clone(),
            view,
        }),
        on_nav,
    )
}

/// A section's header: the triangle, the name in small capitals, and `tail` (a pill, a mark).
fn section_head(label: &str, open: bool, tail: Vec<Dom>) -> Dom {
    let mut head = Dom::create_div()
        .with_class(ids::SECTION_CLASS)
        .with_css(SECTION_HEAD)
        .with_accessibility_name(label)
        .with_child(block(
            TRIANGLE,
            text(if open { "\u{25bc}" } else { "\u{25b6}" }),
        ))
        .with_child(block(CLIP, text(label.to_uppercase())));
    for dom in tail {
        head.add_child(dom);
    }
    head
}

/// ARTICLES: all, unread, starred, read later, the feeds that failed.
fn articles_section(s: &NewsApp, app: &RefAny) -> Dom {
    let lib = &s.library;
    let open = s.articles_open;
    let head = section_head("Articles", open, Vec::new())
        .with_id(ids::SECTION_ARTICLES)
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            app.clone(),
            on_articles_toggle,
        );
    let mut section = Dom::create_div().with_child(head);
    if !open {
        return section;
    }
    let unread = lib.unread_total();
    let rows = [
        (("inbox", "All Articles", unread), View::All, ids::VIEW_ALL),
        (
            ("mark_email_unread", "Unread", unread),
            View::Unread,
            ids::VIEW_UNREAD,
        ),
        (
            ("star", "Starred", lib.starred_count()),
            View::Starred,
            ids::VIEW_STARRED,
        ),
        (
            ("bookmark", "Read Later", lib.later_count()),
            View::Later,
            ids::VIEW_LATER,
        ),
    ];
    for (what, view, id) in rows {
        section.add_child(nav_row(s, app, what, view, id, 20));
    }
    let broken = lib.broken().len();
    if broken > 0 {
        section.add_child(nav_row(
            s,
            app,
            ("error", "Broken Feeds", broken),
            View::Broken,
            ids::VIEW_BROKEN,
            20,
        ));
    }
    section
}

/// What a source's header and its menu carry: the app, the feed (by id) and what to do.
struct SourceRef {
    app: RefAny,
    feed: String,
    action: SourceAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceAction {
    /// Open or close the section.
    Toggle,
    Refresh,
    MarkRead,
    /// Its page: name, folder, unsubscribe.
    Edit,
    /// Follow (`true`) or stop following.
    Follow(bool),
    /// Its page with "Unsubscribe?" asked.
    Remove,
}

fn source_ref(app: &RefAny, feed: &str, action: SourceAction) -> RefAny {
    RefAny::new(SourceRef {
        app: app.clone(),
        feed: feed.to_string(),
        action,
    })
}

/// A source's context menu.
fn source_menu(app: &RefAny, feed: &str, paused: bool) -> Menu {
    let item = |label: &str, action: SourceAction| {
        MenuItem::string(
            StringMenuItem::create(label).with_callback(source_ref(app, feed, action), on_source),
        )
    };
    Menu::create(vec![
        item("Refresh", SourceAction::Refresh),
        item("Mark All as Read", SourceAction::MarkRead),
        MenuItem::separator(),
        item("Edit\u{2026}", SourceAction::Edit),
        if paused {
            item("Follow", SourceAction::Follow(true))
        } else {
            item("Stop Following", SourceAction::Follow(false))
        },
        item("Remove\u{2026}", SourceAction::Remove),
    ])
}

/// A source: its header (closed: with its unread pill), and open: "All" and its topics.
fn source_section(s: &NewsApp, app: &RefAny, i: usize) -> Dom {
    let lib = &s.library;
    let f = &lib.feeds[i];
    let id = f.sub.id.as_str();
    let open = s.source_is_open(id);
    let unread = lib.unread(i);
    let mut tail = Vec::new();
    if !f.meta.error.is_empty() {
        tail.push(
            Dom::create_icon("error")
                .with_css("font-size: 13px; flex-shrink: 0; color: #c0392b;"),
        );
    }
    if f.sub.paused {
        tail.push(block(
            "flex-shrink: 0; margin-left: 4px; font-size: 9px; font-weight: 400;",
            text("paused"),
        ));
    }
    if !open {
        if let Some(badge) = pill(unread, false) {
            tail.push(badge);
        }
    }
    let head = section_head(f.name(), open, tail)
        .with_id(ids::section(i))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            source_ref(app, id, SourceAction::Toggle),
            on_source,
        )
        .with_context_menu(source_menu(app, id, f.sub.paused));
    let mut section = Dom::create_div().with_child(head);
    if f.sub.paused {
        section = section.with_css("opacity: 0.6;");
    }
    if !open {
        return section;
    }
    section.add_child(nav_row(
        s,
        app,
        ("rss_feed", "All", unread),
        View::Feed(f.sub.id.clone()),
        ids::source_all(i),
        20,
    ));
    for (j, topic) in lib.topics(i).iter().enumerate() {
        section.add_child(nav_row(
            s,
            app,
            ("label", topic.name.as_str(), topic.unread),
            View::Topic(f.sub.id.clone(), topic.name.clone()),
            ids::topic(i, j),
            32,
        ));
    }
    section
}

/// NEWS ACTIVITY: what the refresh is doing, when the feeds were last asked, the last word.
fn activity(s: &NewsApp) -> Dom {
    let line = |content: String| block(ACTIVITY_LINE, text(content));
    let mut children = vec![block(ACTIVITY_HEAD, text("NEWS ACTIVITY"))];
    if s.refreshing > 0 {
        let total = s.refresh_total.max(s.refreshing);
        let done = total - s.refreshing;
        children.push(line("Fetching feeds\u{2026}".to_string()));
        children.push(block(
            "padding: 2px 10px;",
            ProgressBar::create(done as f32 * 100.0 / total.max(1) as f32).dom(),
        ));
        children.push(line(format!("{done} of {total} feeds")));
    } else {
        children.push(line(updated_line(s.last_refresh, now_secs())));
    }
    children.push(line(format!(
        "{} unread \u{b7} {} of {} feeds followed",
        s.library.unread_total(),
        s.library.followed().len(),
        s.library.feeds.len()
    )));
    if !s.notice.is_empty() {
        children.push(
            block(
                "padding: 3px 10px 0px 10px; font-size: 11px; font-style: italic;",
                text(s.notice.as_str()),
            )
            .with_accessibility_name(s.notice.as_str()),
        );
    }
    column(ACTIVITY, children).with_id(ids::ACTIVITY)
}

/// One of the small buttons under the list.
fn small_button(icon: &str, label: &str, id: AzString, data: RefAny, cb: CallbackType) -> Dom {
    Dom::create_div()
        .with_id(id)
        .with_css(SMALL_BUTTON)
        .with_accessibility_name(label)
        .with_child(Dom::create_icon(icon))
        .with_callback(EventFilter::Hover(HoverEventFilter::Click), data, cb)
}

/// +, the activity area on / off, the actions menu.
fn bottom_bar(s: &NewsApp, app: &RefAny) -> Dom {
    Dom::create_div()
        .with_css(BOTTOM)
        .with_child(small_button(
            "add",
            "Add a feed",
            ids::NAV_ADD,
            app.clone(),
            on_add_open,
        ))
        .with_child(small_button(
            "data_usage",
            if s.show_activity {
                "Hide the activity"
            } else {
                "Show the activity"
            },
            ids::NAV_ACTIVITY,
            app.clone(),
            on_activity_toggle,
        ))
        .with_child(small_button(
            "settings",
            "Actions",
            ids::NAV_ACTIONS,
            app.clone(),
            on_actions,
        ))
}

// ==== Callbacks ====

extern "C" fn on_nav(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, view)) = data
        .downcast_ref::<NavRef>()
        .map(|r| (r.app.clone(), r.view.clone()))
    else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, info, handle| {
        super::leave_form(s, info, handle);
        set_view(s, view);
    })
}

extern "C" fn on_articles_toggle(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _, _| {
        s.articles_open = !s.articles_open
    })
}

extern "C" fn on_activity_toggle(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _, _| {
        s.show_activity = !s.show_activity
    })
}

/// The actions menu (Mail's gear): sources, import, export, refresh, settings.
extern "C" fn on_actions(data: RefAny, mut info: CallbackInfo) -> Update {
    let item = |label: &str, cb: CallbackType| {
        MenuItem::string(StringMenuItem::create(label).with_callback(data.clone(), cb))
    };
    let menu = Menu::create(vec![
        item("Manage Sources\u{2026}", on_sources_open),
        item("Add Feed\u{2026}", on_add_open),
        item("Import Subscriptions (OPML)\u{2026}", on_import_open),
        item("Export Subscriptions (OPML)", on_export),
        MenuItem::separator(),
        item("Get News", on_refresh),
        item("Settings\u{2026}", on_open_settings),
    ]);
    info.open_menu_for_hit_node(menu);
    Update::DoNothing
}

/// A source's header was clicked (open / close) or one of its menu's entries picked.
extern "C" fn on_source(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, feed, action)) = data
        .downcast_ref::<SourceRef>()
        .map(|r| (r.app.clone(), r.feed.clone(), r.action))
    else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, info, handle| {
        let Some(i) = s.library.feed_index(&feed) else {
            return;
        };
        match action {
            SourceAction::Toggle => {
                let open = s.source_is_open(&feed);
                s.source_open.insert(feed, !open);
            }
            SourceAction::Refresh => start_refresh(s, info, handle, vec![i]),
            SourceAction::MarkRead => {
                let refs = s.library.list(&View::Feed(feed), "");
                let unread = mark_read(s, info, handle, &refs);
                println!("AZNEWS_MARKED_ALL {unread}");
                s.notice = format!("{unread} article(s) marked as read");
            }
            SourceAction::Edit => {
                super::leave_form(s, info, handle);
                s.reading = Reading::Feed(feed);
            }
            SourceAction::Follow(on) => set_followed(s, info, handle, i, on),
            SourceAction::Remove => {
                super::leave_form(s, info, handle);
                s.reading = Reading::Feed(feed);
                s.confirm_unsubscribe = true;
            }
        }
    })
}
