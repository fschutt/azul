//! The sources page - which feeds the user follows: every subscription with a switch (a feed
//! not followed keeps its articles but is not refreshed and stays out of "All articles"), its
//! site, folder, articles and how its last refresh went, Edit (its page: name, folder) and
//! Remove (asked first; its articles go with it); Add Feed, Import / Export OPML and Get News
//! at the top.

use azul::{
    callbacks::{ButtonOnClickCallbackType, SwitchOnToggleCallbackType},
    prelude::*,
    widgets::{ButtonType, Switch, SwitchState},
};
use azul_appkit::{
    pieces::{block, button, column, flex_row, primary, text},
    ui as kit,
};

use super::{
    age, form_page, now_secs, on_add_open, on_export, on_import_open, on_leave, on_refresh,
    set_followed, unsubscribe, with_app, NewsApp, Reading,
};
use crate::{ids, library::FeedData, links};

const NAME: &str = "font-size: 13px; font-weight: 600; overflow: hidden; white-space: nowrap; \
                    text-overflow: ellipsis;";
const DETAIL: &str = "font-size: 11px; opacity: 0.7; overflow: hidden; white-space: nowrap; \
                      text-overflow: ellipsis;";
const ERROR: &str = "font-size: 11px; color: #b3261e; \
                     @media (prefers-color-scheme: dark) { color: #f2b8b5; }";

/// The page.
pub(super) fn sources_page(s: &NewsApp, app: &RefAny) -> Dom {
    let lib = &s.library;
    let mut children = vec![
        block(
            "font-size: 18px; font-weight: 600; padding-bottom: 4px;",
            text("Sources"),
        ),
        kit::note(&format!(
            "{} feeds, {} followed. A feed you do not follow keeps its articles, but it is not \
             refreshed and its articles stay out of All articles and Unread.",
            lib.feeds.len(),
            lib.followed().len()
        )),
        flex_row(
            "gap: 6px; padding: 8px 0px; flex-wrap: wrap;",
            vec![
                primary("Add Feed\u{2026}", ids::SOURCES_ADD, app, on_add_open),
                button("Import OPML\u{2026}", ids::SOURCES_IMPORT, app, on_import_open),
                button("Export OPML", ids::SOURCES_EXPORT, app, on_export),
                button("Get News", ids::SOURCES_REFRESH, app, on_refresh),
                block("flex-grow: 1;", Dom::create_div()),
                button("Done", ids::SOURCES_DONE, app, on_leave),
            ],
        ),
    ];
    for (i, f) in lib.feeds.iter().enumerate() {
        children.push(source_row(s, app, i, f));
    }
    form_page(children).with_id(ids::SOURCES)
}

/// What a row's controls carry: the app and the feed (by id).
struct SourceRowRef {
    app: RefAny,
    feed: String,
}

fn row_ref(app: &RefAny, f: &FeedData) -> RefAny {
    RefAny::new(SourceRowRef {
        app: app.clone(),
        feed: f.sub.id.clone(),
    })
}

/// A button of a row.
fn row_button(
    label: &str,
    kind: Option<ButtonType>,
    data: RefAny,
    cb: ButtonOnClickCallbackType,
    id: azul::str::String,
) -> Dom {
    let mut b = Button::create(label);
    if let Some(kind) = kind {
        b = b.with_button_type(kind);
    }
    b.with_on_click(data, cb).dom().with_id(id)
}

/// One feed: its switch, its name and facts, Edit and Remove (or "Remove?").
fn source_row(s: &NewsApp, app: &RefAny, i: usize, f: &FeedData) -> Dom {
    let lib = &s.library;
    let follow = Switch::create(!f.sub.paused)
        .with_on_toggle(row_ref(app, f), on_follow as SwitchOnToggleCallbackType)
        .dom()
        .with_id(ids::source_follow(i));
    let site = Some(links::site_name(&f.sub.url))
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| f.sub.url.clone());
    let folder = if f.sub.folder.is_empty() {
        "no folder".to_string()
    } else {
        f.sub.folder.clone()
    };
    let facts = format!(
        "{site} \u{b7} {folder} \u{b7} {} articles, {} unread",
        f.items.len(),
        lib.unread(i)
    );
    let status = if !f.meta.error.is_empty() {
        block(ERROR, text(format!("The last refresh failed: {}", f.meta.error)))
    } else if f.meta.checked == 0 {
        block(DETAIL, text("Not refreshed yet"))
    } else {
        block(
            DETAIL,
            text(format!(
                "Asked {} ago{}",
                age(f.meta.checked, now_secs()),
                if f.sub.paused { " \u{b7} not followed" } else { "" }
            )),
        )
    };
    let info = column(
        "flex-grow: 1; min-width: 0px;",
        vec![
            block(NAME, text(f.name())),
            block(DETAIL, text(facts)),
            status,
        ],
    );
    let asked = s.confirm_remove.as_deref() == Some(f.sub.id.as_str());
    let actions = if asked {
        vec![
            row_button(
                "Remove and delete its articles",
                Some(ButtonType::Danger),
                row_ref(app, f),
                on_remove_yes,
                ids::source_remove_yes(i),
            ),
            row_button(
                "Cancel",
                None,
                row_ref(app, f),
                on_remove_no,
                ids::source_remove_no(i),
            ),
        ]
    } else {
        vec![
            row_button(
                "Edit\u{2026}",
                None,
                row_ref(app, f),
                on_edit,
                ids::source_edit(i),
            ),
            row_button(
                "Remove\u{2026}",
                None,
                row_ref(app, f),
                on_remove,
                ids::source_remove(i),
            ),
        ]
    };
    let mut children = vec![follow, info];
    children.extend(actions);
    flex_row(
        "gap: 10px; padding: 6px 0px; border-bottom: 1px solid system:separator;",
        children,
    )
    .with_id(ids::source_row(i))
    .with_accessibility_name(f.name())
}

fn row_parts(data: &mut RefAny) -> Option<(RefAny, String)> {
    data.downcast_ref::<SourceRowRef>()
        .map(|r| (r.app.clone(), r.feed.clone()))
}

/// The switch: follow the feed, or stop following it.
extern "C" fn on_follow(mut data: RefAny, mut info: CallbackInfo, state: SwitchState) -> Update {
    let Some((mut app, feed)) = row_parts(&mut data) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, info, handle| {
        if let Some(i) = s.library.feed_index(&feed) {
            set_followed(s, info, handle, i, state.checked);
        }
    })
}

/// Edit: the feed's page (name, folder, refresh, unsubscribe).
extern "C" fn on_edit(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, feed)) = row_parts(&mut data) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, _info, _| {
        s.confirm_remove = None;
        s.reading = Reading::Feed(feed);
    })
}

/// Remove: asked first.
extern "C" fn on_remove(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, feed)) = row_parts(&mut data) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, _info, _| {
        s.confirm_remove = Some(feed)
    })
}

extern "C" fn on_remove_no(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, _)) = row_parts(&mut data) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, _info, _| s.confirm_remove = None)
}

/// "Remove and delete its articles": unsubscribed, its files deleted, the page stays.
extern "C" fn on_remove_yes(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, feed)) = row_parts(&mut data) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, info, handle| {
        unsubscribe(s, info, handle, &feed);
        s.notice = "The feed was removed.".to_string();
    })
}
