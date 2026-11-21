//! The location tabs in the window, over the model in `location_tabs`: Ctrl+T opens a tab on
//! the folder the window shows, Ctrl+W closes the tab showing (never the last), Ctrl+Tab and
//! Ctrl+Shift+Tab cycle, Ctrl+1..9 jump, and the tab overview (the strip's tab button or
//! Ctrl+Shift+A) lists them over a grey scrim: a row per tab (its place's icon, its name, the
//! folders above it, its Ctrl digit, a close box), the tab showing marked, and a New tab row.
//! The tabs are not drawn as a strip; the overview and the count on the strip's tab button are
//! their only UI.
//!
//! A switch parks the window's live state (place, history, selection, layout, search, scroll)
//! and shows the target's: its place is listed again, its search runs again, and its view
//! scrolls back once the folder is listed (`DriveState::scroll_when_listed`).
//!
//! On stdout, for scripts: `AZDRIVE_TABS <count> <index of the tab showing>` after every change,
//! `AZDRIVE_TAB_OVERVIEW open|closed`.

use azul::{
    callbacks::TabOverviewOnEventCallbackType,
    prelude::*,
    str::String as AzString,
    vec::TabOverviewItemVec,
    widgets::{TabOverview, TabOverviewEvent, TabOverviewEventKind, TabOverviewItem},
};
use azul_appkit::l10n::{label, t_args, Arg, Phrase, Text};

use crate::{
    actions,
    browse::{self, Place},
    go, ids,
    location_tabs::{shortcut_digit, Closed, TabState},
    with_state, DriveState,
};

/// The window's live state as a tab keeps it.
pub(crate) fn live_tab(s: &DriveState) -> TabState {
    TabState {
        place: s.place.clone(),
        history: s.history.clone(),
        selection: s.selection.clone(),
        layout: s.settings.layout,
        search: if s.search_open {
            s.search.clone()
        } else {
            String::new()
        },
        scroll: s.view_scroll.0,
    }
}

/// Shows `tab` in the window: its place listed again, then its history, selection, layout and
/// search; the view scrolls back once the folder is listed.
fn show_tab(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, tab: TabState) {
    go(info, app, s, tab.place, false);
    s.history = tab.history;
    s.selection = tab.selection;
    if s.settings.layout != tab.layout {
        s.settings.layout = tab.layout;
        println!("AZDRIVE_LAYOUT {}", tab.layout.name());
    }
    if !tab.search.is_empty() {
        s.search = tab.search;
        actions::set_search_box_text(info, &s.search);
        crate::start_find(info, app, s);
    }
    s.scroll_when_listed = (tab.scroll > 0.0).then_some(tab.scroll);
    s.print_selection();
    print_tabs(s);
}

/// Prints the tabs for scripts.
pub(crate) fn print_tabs(s: &DriveState) {
    println!("AZDRIVE_TABS {} {}", s.tabs.len(), s.tabs.current());
}

/// Ctrl+T, the File menu's New tab, the overview's New tab row: a tab on the folder the window
/// shows, after the one showing; a note says where it opened.
pub(crate) fn new_tab(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let live = live_tab(s);
    let fresh = s.tabs.open(live);
    s.tab_overview = false;
    show_tab(info, app, s, fresh);
    s.info(Phrase::new("azdrive-loctab-opened").arg("place", s.place_name().as_str()));
}

/// A folder's "Open in new tab": a new tab, which then goes to `place` (Back returns to the
/// folder it opened on).
pub(crate) fn open_in_new_tab(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    place: Place,
) {
    new_tab(info, app, s);
    go(info, app, s, place, true);
    s.info(Phrase::new("azdrive-loctab-opened").arg("place", s.place_name().as_str()));
}

/// Shows tab `to` (nothing for the tab showing or one past the last).
pub(crate) fn switch_tab(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, to: usize) {
    let live = live_tab(s);
    if let Some(tab) = s.tabs.switch(to, live) {
        show_tab(info, app, s, tab);
    }
}

/// Ctrl+W (the tab showing) or the overview's close box (`at`): the last tab stays, with a
/// note; closing the tab showing shows the one that takes its place.
pub(crate) fn close_tab(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, at: usize) {
    let live = live_tab(s);
    match s.tabs.close(at, live) {
        Closed::LastTab => s.info(Text::key("azdrive-loctab-last")),
        Closed::Other => print_tabs(s),
        Closed::Current(tab) => show_tab(info, app, s, tab),
    }
    if s.tabs.len() < 2 {
        s.tab_overview = false;
    }
}

/// The tab overview opens, or closes when it is open.
pub(crate) fn toggle_overview(s: &mut DriveState) {
    s.tab_overview = !s.tab_overview;
    s.tab_overview_highlight = usize::MAX;
    println!(
        "AZDRIVE_TAB_OVERVIEW {}",
        if s.tab_overview { "open" } else { "closed" }
    );
}

/// What the overview calls a tab: its search ("Search: steuer"), else its place.
pub(crate) fn tab_title(s: &DriveState, tab: &TabState) -> String {
    if tab.search.is_empty() {
        s.place_title(&tab.place)
    } else {
        t_args("azdrive-loctab-search", &[("text", Arg::from(tab.search.as_str()))])
    }
}

/// The folders above a tab's place, from its drive (`Home › Documents`); empty for a drive's
/// root, This PC and Quick access.
pub(crate) fn tab_path(s: &DriveState, tab: &TabState) -> String {
    let Place::Folder { .. } = &tab.place else {
        return String::new();
    };
    let drive_name = s.drive_name(&tab.place);
    let crumbs = browse::crumbs_of(&tab.place, &drive_name);
    // The place itself last: it is not "above" itself.
    let above: Vec<String> = crumbs.into_iter().map(|(name, _)| name).collect::<Vec<String>>();
    match above.split_last() {
        Some((_, parents)) => parents.join(" \u{203A} "),
        None => String::new(),
    }
}

/// A tab's icon in the overview: a search, This PC, Quick access, a drive, a folder.
fn tab_icon(s: &DriveState, tab: &TabState) -> &'static str {
    if !tab.search.is_empty() {
        return "search";
    }
    match &tab.place {
        Place::QuickAccess => "star",
        Place::ThisPc => "computer",
        Place::Folder { drive, prefix } if prefix.is_empty() => s
            .slot_index(drive)
            .map_or("storage", |i| s.slots[i].icon()),
        Place::Folder { .. } => "folder",
    }
}

/// The tab overview, while it shows: the tabs as the window has them (the one showing with its
/// live state), over the grey scrim; `None` while it is closed.
pub(crate) fn overview(s: &DriveState, app: &RefAny) -> Option<Dom> {
    if !s.tab_overview {
        return None;
    }
    let rows: Vec<TabOverviewItem> = s
        .tabs
        .snapshot(live_tab(s))
        .iter()
        .enumerate()
        .map(|(index, tab)| {
            let digit = shortcut_digit(index).map_or_else(String::new, |d| format!("\u{2303}{d}"));
            TabOverviewItem::create(
                AzString::from(tab_icon(s, tab)),
                AzString::from(tab_title(s, tab)),
                AzString::from(tab_path(s, tab)),
                AzString::from(digit),
            )
        })
        .collect();
    Some(
        TabOverview::create(TabOverviewItemVec::from(rows), s.tabs.current())
            .with_highlighted(s.tab_overview_highlight)
            .with_words(
                label("azdrive-loctabs-title"),
                label("azdrive-loctab-new"),
                AzString::from("\u{2303}T"),
                label("azdrive-loctabs-hint"),
            )
            .with_on_event(app.clone(), on_overview as TabOverviewOnEventCallbackType)
            .dom()
            .with_id(ids::TAB_OVERVIEW),
    )
}

/// A row of the overview picked (a click, Return), its close box, its New tab row, or the
/// overview dismissed (the scrim, Escape, the strip's button again).
extern "C" fn on_overview(mut data: RefAny, mut info: CallbackInfo, event: TabOverviewEvent) -> Update {
    with_state(&mut data, &mut info, |info, app, s| match event.kind {
        TabOverviewEventKind::Pick => {
            s.tab_overview = false;
            switch_tab(info, app, s, event.index);
        }
        TabOverviewEventKind::Close => close_tab(info, app, s, event.index),
        TabOverviewEventKind::NewTab => new_tab(info, app, s),
        // The keyboard moved the highlight: the window is drawn again with it.
        TabOverviewEventKind::Highlight => s.tab_overview_highlight = event.index,
        TabOverviewEventKind::Dismiss => {
            s.tab_overview = false;
            println!("AZDRIVE_TAB_OVERVIEW closed");
        }
    })
}

/// The strip's tab button's words: how many tabs are open, with the overview's key.
pub(crate) fn count_label(s: &DriveState) -> String {
    format!(
        "{} (\u{2303}\u{21E7}A)",
        t_args("azdrive-loctabs-count", &[("count", Arg::from(s.tabs.len()))])
    )
}
