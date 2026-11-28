//! The thin toolbar of the reader and of the library, like the Chrome PDF viewer's: icons only
//! (a control shows a word or an icon, never both), each named for a screen reader and as its
//! tooltip; the title, the page field and the zoom are the words in it.
//!
//! The reader's, on a desktop: the strip's toggle, the book's title, the page (n of N, a field
//! to type one in, the arrows), the zoom, the layout (fit), a bookmark, a copy of the book,
//! the chapter as a PDF (to print), Settings, the library. On a phone only what a thumb needs
//! while it reads (back to the library, the title, the zoom, a bookmark, Settings): the pages
//! turn from the page's sides and the slider under it, and the rest is in Settings. Nothing
//! there is moved into the toolbar's "more" menu: a menu is a window, which a phone has none of.
//!
//! The library's: Add a book, the search field, the sort (a button that goes round Recent,
//! Title, Author), Settings.

use azul::{
    callbacks::{
        CallbackInfo, RefAny, TextInputOnTextInputCallbackType,
        TextInputOnVirtualKeyDownCallbackType, ToolbarOnEventCallbackType, Update,
    },
    dom::{Dom, VirtualKeyCode},
    str::String as AzString,
    widgets::{
        OnTextInputReturn, TextInput, TextInputState, TextInputValid, Toolbar, ToolbarEvent,
        ToolbarEventKind, ToolbarItem,
    },
};
use azul_appkit::l10n::{t, t_args, Arg};

use crate::{
    app::{AppState, Command, Pane},
    chrome,
    commands::run,
    ids,
    library::Sort,
};

/// The title in the toolbar: one line, cut with an ellipsis when it does not fit.
fn title_label(title: &str) -> Dom {
    Dom::create_div()
        .with_css(
            "overflow: hidden; white-space: nowrap; text-overflow: ellipsis; font-size: 14px; \
             font-weight: 600; padding: 0px 8px;",
        )
        .with_child(Dom::create_span_with_text(title))
}

/// The page field: the page on screen, Return goes to the page typed.
fn page_field(app: &RefAny, page: usize) -> Dom {
    TextInput::create()
        .with_text(page.to_string().as_str())
        .with_accessibility_name(t("azreader-page"))
        .with_on_virtual_key_down(
            app.clone(),
            on_page_key as TextInputOnVirtualKeyDownCallbackType,
        )
        .dom()
        .with_id(ids::TB_PAGE)
}

/// "/ 12": how many pages the chapter has.
fn page_count_label(count: usize) -> Dom {
    Dom::create_span_with_text(format!("/ {count}").as_str())
        .with_id(ids::TB_COUNT)
        .with_css("font-size: 12px; opacity: 0.7;")
}

/// The percent the text is zoomed to.
fn zoom_label(font_px: u32) -> Dom {
    Dom::create_span_with_text(format!("{} %", chrome::zoom_percent(font_px)).as_str())
        .with_id(ids::TB_ZOOM)
        .with_css("font-size: 12px; min-width: 40px; text-align: center;")
}

/// The reader's toolbar, for the window `st` was laid out in.
#[must_use]
pub fn reader(app: &RefAny, st: &AppState) -> Dom {
    let items = if st.phone {
        reader_items_phone(st)
    } else {
        reader_items_desktop(app, st)
    };
    st.toolbar_fit
        .apply(Toolbar::create(t("azreader-toolbar")).with_items(items))
        .with_on_event(app.clone(), on_toolbar as ToolbarOnEventCallbackType)
        .dom()
        .with_id(ids::TOOLBAR)
}

/// The title of the open book (empty without one).
fn book_title(st: &AppState) -> String {
    st.open
        .as_ref()
        .map_or_else(String::new, |o| o.info.title.clone())
}

/// A bookmark is on the page on screen.
fn page_is_marked(st: &AppState) -> bool {
    st.open
        .as_ref()
        .zip(st.position())
        .is_some_and(|(open, at)| open.state.bookmark_at(at).is_some())
}

/// The bookmark tool: on while the page is marked.
fn bookmark_tool(st: &AppState) -> ToolbarItem {
    let marked = page_is_marked(st);
    ToolbarItem::create_toggle(
        ids::TB_BOOKMARK,
        t(if marked { "azreader-remove-bookmark" } else { "azreader-bookmark-page" }),
        if marked { "bookmark" } else { "bookmark_border" },
        marked,
    )
}

/// The tools of a desktop's reader bar, in their order.
fn reader_items_desktop(app: &RefAny, st: &AppState) -> Vec<ToolbarItem> {
    let count = st.chapter.as_ref().map_or(0, |c| c.pages.page_count());
    let page = chrome::page_number(st.page, count);
    let (low, high) = chrome::zoom_range();
    let zoom = chrome::zoom_percent(st.settings.font_px);
    let mut prev = ToolbarItem::create_button(ids::TB_PREV, t("azreader-previous-page"), "navigate_before");
    if page <= 1 {
        prev = prev.with_disabled(t("azreader-first-page-now"));
    }
    let mut next = ToolbarItem::create_button(ids::TB_NEXT, t("azreader-next-page"), "navigate_next");
    if count > 0 && page >= count {
        next = next.with_disabled(t("azreader-last-page-now"));
    }
    let mut zoom_out = ToolbarItem::create_button(ids::TB_ZOOM_OUT, t("azreader-zoom-out"), "remove");
    if zoom <= low {
        zoom_out = zoom_out.with_disabled(t("azreader-smallest-text"));
    }
    let mut zoom_in = ToolbarItem::create_button(ids::TB_ZOOM_IN, t("azreader-zoom-in"), "add");
    if zoom >= high {
        zoom_in = zoom_in.with_disabled(t("azreader-largest-text"));
    }
    let fit = ToolbarItem::create_button(
        ids::TB_FIT,
        t_args("azreader-fit-layout", &[("layout", Arg::from(t(st.settings.layout.label())))]),
        chrome::layout_icon(st.settings.layout),
    );
    vec![
        ToolbarItem::create_toggle(ids::TB_STRIP, t("azreader-strip"), "menu", st.pane != Pane::None),
        ToolbarItem::create_custom(ids::TB_TITLE, t("azreader-title"), title_label(&book_title(st)), 260.0),
        ToolbarItem::create_spacer(),
        prev,
        ToolbarItem::create_custom(ids::TB_PAGE, t("azreader-page"), page_field(app, page), 56.0),
        ToolbarItem::create_custom(ids::TB_COUNT, t("azreader-page-count-name"), page_count_label(count), 44.0),
        next,
        ToolbarItem::create_separator(),
        zoom_out,
        ToolbarItem::create_custom(ids::TB_ZOOM, t("azreader-zoom"), zoom_label(st.settings.font_px), 48.0),
        zoom_in,
        fit,
        ToolbarItem::create_separator(),
        bookmark_tool(st),
        ToolbarItem::create_button(ids::TB_DOWNLOAD, t("azreader-download"), "download"),
        ToolbarItem::create_button(ids::TB_PRINT, t("azreader-print"), "print"),
        ToolbarItem::create_button(ids::TB_SETTINGS, t("azreader-settings"), "tune"),
        ToolbarItem::create_button(ids::TB_LIBRARY, t("azreader-library"), "local_library"),
    ]
}

/// The tools of a phone's reader bar: back to the library, the title, the zoom, a bookmark,
/// Settings. They all fit a phone's width, so none is moved into a menu.
fn reader_items_phone(st: &AppState) -> Vec<ToolbarItem> {
    let (low, high) = chrome::zoom_range();
    let zoom = chrome::zoom_percent(st.settings.font_px);
    let mut zoom_out = ToolbarItem::create_button(ids::TB_ZOOM_OUT, t("azreader-zoom-out"), "remove");
    if zoom <= low {
        zoom_out = zoom_out.with_disabled(t("azreader-smallest-text"));
    }
    let mut zoom_in = ToolbarItem::create_button(ids::TB_ZOOM_IN, t("azreader-zoom-in"), "add");
    if zoom >= high {
        zoom_in = zoom_in.with_disabled(t("azreader-largest-text"));
    }
    vec![
        ToolbarItem::create_button(ids::TB_LIBRARY, t("azreader-library"), "arrow_back").with_never_overflow(true),
        ToolbarItem::create_custom(ids::TB_TITLE, t("azreader-title"), title_label(&book_title(st)), 120.0),
        ToolbarItem::create_spacer(),
        zoom_out.with_never_overflow(true),
        zoom_in.with_never_overflow(true),
        bookmark_tool(st).with_never_overflow(true),
        ToolbarItem::create_button(ids::TB_SETTINGS, t("azreader-settings"), "tune").with_never_overflow(true),
    ]
}

/// The library's toolbar.
#[must_use]
pub fn library(app: &RefAny, st: &AppState) -> Dom {
    let search = TextInput::create_search()
        .with_text(st.query.as_str())
        .with_placeholder(t("azreader-search"))
        .with_accessibility_name(t("azreader-search-library"))
        .with_on_text_input(app.clone(), crate::ui_library::on_search_text as TextInputOnTextInputCallbackType)
        .with_on_virtual_key_down(app.clone(), crate::ui_library::on_search_key as TextInputOnVirtualKeyDownCallbackType)
        .dom()
        .with_id(ids::LIBRARY_SEARCH);
    let mut items = vec![
        ToolbarItem::create_button(ids::TB_ADD, t("azreader-add-book"), "library_add").with_never_overflow(true),
        ToolbarItem::create_custom(ids::TB_SEARCH, t("azreader-search-library"), search, if st.phone { 180.0 } else { 300.0 })
            .with_never_overflow(true),
        ToolbarItem::create_spacer(),
        ToolbarItem::create_button(
            ids::TB_SORT,
            t_args("azreader-sort-by", &[("sort", Arg::from(t(st.sort.label())))]),
            sort_icon(st.sort),
        )
        .with_never_overflow(true),
    ];
    if st.open.is_some() {
        items.push(
            ToolbarItem::create_button(ids::TB_LIBRARY, t("azreader-back-to-book"), "auto_stories")
                .with_never_overflow(true),
        );
    }
    items.push(ToolbarItem::create_button(ids::TB_SETTINGS, t("azreader-settings"), "tune").with_never_overflow(true));
    st.toolbar_fit
        .apply(Toolbar::create(t("azreader-library")).with_items(items))
        .with_on_event(app.clone(), on_toolbar as ToolbarOnEventCallbackType)
        .dom()
        .with_id(ids::TOOLBAR)
}

/// The icon of a sort.
#[must_use]
pub const fn sort_icon(sort: Sort) -> &'static str {
    match sort {
        Sort::Recent => "schedule",
        Sort::Title => "sort_by_alpha",
        Sort::Author => "person",
    }
}

/// The sort after `sort`: Recent, Title, Author, Recent.
#[must_use]
pub const fn next_sort(sort: Sort) -> Sort {
    match sort {
        Sort::Recent => Sort::Title,
        Sort::Title => Sort::Author,
        Sort::Author => Sort::Recent,
    }
}

/// The command a tool of the toolbar runs, from its `id`: `None` for a tool that is no command.
/// `pane` is the strip's state (its toggle opens the pages, or closes whatever is open), `sort`
/// the sort the sort button is on (it goes to the next).
#[must_use]
pub fn command_of(id: &str, pane: Pane, sort: Sort) -> Option<Command> {
    let is = |name: &AzString| id == name.as_str();
    Some(if is(&ids::TB_STRIP) {
        Command::Pane(if pane == Pane::None { Pane::Pages } else { pane })
    } else if is(&ids::TB_PREV) {
        Command::PrevPage
    } else if is(&ids::TB_NEXT) {
        Command::NextPage
    } else if is(&ids::TB_ZOOM_OUT) {
        Command::FontSize(-2)
    } else if is(&ids::TB_ZOOM_IN) {
        Command::FontSize(2)
    } else if is(&ids::TB_FIT) {
        Command::CycleLayout
    } else if is(&ids::TB_BOOKMARK) {
        Command::ToggleBookmark
    } else if is(&ids::TB_DOWNLOAD) {
        Command::Download
    } else if is(&ids::TB_PRINT) {
        Command::Print
    } else if is(&ids::TB_SETTINGS) {
        Command::Settings
    } else if is(&ids::TB_LIBRARY) {
        Command::ShowLibrary
    } else if is(&ids::TB_ADD) {
        Command::AddBooks
    } else if is(&ids::TB_SORT) {
        Command::Sort(next_sort(sort))
    } else {
        return None;
    })
}

/// A tool was pressed: the tool's `id` names the command.
extern "C" fn on_toolbar(mut data: RefAny, mut info: CallbackInfo, event: ToolbarEvent) -> Update {
    let (pane, sort, screen_is_library) = {
        let Some(mut st) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        // The bar measured itself: it is built again to fit when that changed.
        if st.toolbar_fit.take(&event) {
            return Update::RefreshDom;
        }
        (st.pane, st.sort, st.screen == crate::app::Screen::Library)
    };
    if !matches!(event.kind, ToolbarEventKind::Activate | ToolbarEventKind::Toggle) {
        return Update::DoNothing;
    }
    let Some(mut command) = command_of(event.id.as_str(), pane, sort) else {
        return Update::DoNothing;
    };
    // In the library the library button is "back to the book".
    if screen_is_library && command == Command::ShowLibrary {
        command = Command::ShowReader;
    }
    run(&mut data, command, &mut info)
}

/// Return in the page field: that page of the chapter.
pub extern "C" fn on_page_key(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let is_return = matches!(
        info.get_current_keyboard_state().current_virtual_keycode.into_option(),
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter)
    );
    let mut update = Update::DoNothing;
    if is_return {
        let text = state.get_text().as_str().to_string();
        let count = data
            .downcast_ref::<AppState>()
            .and_then(|st| st.chapter.as_ref().map(|c| c.pages.page_count()))
            .unwrap_or(0);
        if let Some(page) = chrome::typed_page(&text, count) {
            info.stop_propagation();
            update = run(&mut data, Command::GoToPage(page), &mut info);
        }
    }
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

#[cfg(test)]
#[path = "toolbar_test.rs"]
mod toolbar_test;
