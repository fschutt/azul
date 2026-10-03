//! The library: the shelves (the navigation pane), a search field, the covers grid, the status
//! bar. A tile is the book's cover (or a coloured tile with its title when it has none), its
//! title, its author and how far the reader is; a click selects it, a double click opens it.
//!
//! TODO(WIDGETS9A): IconGrid - the grid here is a wrapping row of tiles (no virtualization,
//! no keyboard navigation, no rubber band); WIDGETS9A's IconGrid replaces it.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CallbackInfo, RefAny, TextInputOnTextInputCallbackType,
        TextInputOnVirtualKeyDownCallbackType, Update,
    },
    css::{EventFilter, HoverEventFilter},
    dom::{Dom, VirtualKeyCode},
    shells::ShellEmptyState,
    str::String as AzString,
    widgets::{
        OnTextInputReturn, StatusBar, StatusBarSegment, TextInput, TextInputState, TextInputValid,
    },
};

use crate::{
    app::{command, AppState, Command},
    commands::on_command,
    ids,
    library::{self, LibraryEntry, Shelf},
    position,
};

fn s(text: &str) -> AzString {
    AzString::from(text)
}

/// A tile's colours when the book has no cover (by its title: the same book, the same tile).
const TILE_COLORS: [(&str, &str); 6] = [
    ("#2f4858", "#f2f2f2"),
    ("#6b3e26", "#fbe9d0"),
    ("#33673b", "#eef6e8"),
    ("#4a3b6b", "#efe9fb"),
    ("#8a2f3c", "#fbe8ea"),
    ("#1f5f8b", "#e7f2fb"),
];

fn tile_colors(title: &str) -> (&'static str, &'static str) {
    let sum = title
        .bytes()
        .fold(0_usize, |a, b| a.wrapping_mul(31).wrapping_add(b as usize));
    TILE_COLORS[sum % TILE_COLORS.len()]
}

/// The shelves.
#[must_use]
pub fn navigation(app: &RefAny, st: &AppState) -> Dom {
    let mut pane = Dom::create_div().with_css(
        "display: flex; flex-direction: column; padding: 8px 0px; flex-grow: 1; \
         background: system:window-background;",
    );
    for (i, shelf) in Shelf::ALL.iter().enumerate() {
        let count = st.entries.iter().filter(|e| shelf.holds(e)).count();
        let on = st.shelf == *shelf;
        let mut row = Dom::create_div()
            .with_id(ids::indexed(&ids::SHELF_ROW, i))
            .with_css(if on {
                "display: flex; flex-direction: row; align-items: center; padding: 6px 14px; \
                 background: system:selection-background; color: system:text; cursor: pointer;"
            } else {
                "display: flex; flex-direction: row; align-items: center; padding: 6px 14px; \
                 color: system:text; cursor: pointer;"
            })
            .with_accessibility_name(shelf.label())
            .with_child(
                Dom::create_div()
                    .with_css("flex-grow: 1; font-size: 13px;")
                    .with_child(Dom::create_span_with_text(shelf.label())),
            )
            .with_child(
                Dom::create_div()
                    .with_css("font-size: 12px; color: system:secondary-text;")
                    .with_child(Dom::create_span_with_text(count.to_string())),
            );
        row.add_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            command(app, Command::Shelf(*shelf)),
            on_command,
        );
        pane.add_child(row);
    }
    pane
}

/// One book's tile.
fn tile(app: &RefAny, st: &AppState, index: usize, entry: &LibraryEntry) -> Dom {
    let id = entry.info.id.clone();
    let selected = st.selected.as_deref() == Some(id.as_str());
    let cover = match st.covers.get(&id) {
        Some(image) => Dom::create_image(image.clone()).with_css(
            "width: 120px; height: 180px; flex-shrink: 0; border: 1px solid system:separator;",
        ),
        None => {
            let (background, text) = tile_colors(&entry.info.title);
            Dom::create_div()
                .with_css(format!(
                    "display: flex; flex-direction: column; justify-content: center; width: 120px; \
                     height: 180px; flex-shrink: 0; box-sizing: border-box; padding: 12px; \
                     background: {background}; color: {text}; font-family: serif; \
                     font-size: 14px; text-align: center; overflow: hidden;"
                ))
                .with_child(Dom::create_span_with_text(entry.info.title.as_str()))
        }
    };
    let progress = if entry.state.finished {
        "Finished".to_string()
    } else if entry.state.last_read == 0 {
        "New".to_string()
    } else {
        position::percent_label(entry.state.progress)
    };
    let mut tile = Dom::create_div()
        .with_id(ids::indexed(&ids::BOOK_TILE, index))
        .with_class(ids::BOOK_TILE_CLASS)
        .with_css(format!(
            "display: flex; flex-direction: column; width: 136px; padding: 8px; margin: 6px; \
             border-radius: 4px; cursor: pointer; {}",
            if selected {
                "background: system:selection-background;"
            } else {
                ""
            }
        ))
        .with_accessibility_name(entry.info.title.as_str())
        .with_child(cover)
        .with_child(
            Dom::create_div()
                .with_css("padding-top: 6px; font-size: 13px; font-weight: 600; color: system:text; overflow: hidden; max-height: 34px;")
                .with_child(Dom::create_span_with_text(entry.info.title.as_str())),
        )
        .with_child(
            Dom::create_div()
                .with_css("font-size: 12px; color: system:secondary-text; overflow: hidden; max-height: 16px;")
                .with_child(Dom::create_span_with_text(entry.info.author_line())),
        )
        .with_child(
            Dom::create_div()
                .with_css("font-size: 11px; color: system:tertiary-text;")
                .with_child(Dom::create_span_with_text(progress)),
        );
    tile.add_callback(
        EventFilter::Hover(HoverEventFilter::Click),
        command(app, Command::Select(id.clone())),
        on_command,
    );
    tile.add_callback(
        EventFilter::Hover(HoverEventFilter::DoubleClick),
        command(app, Command::OpenBook(id)),
        on_command,
    );
    tile
}

/// The search field and the covers grid (or the empty state).
#[must_use]
pub fn content(app: &RefAny, st: &AppState) -> Dom {
    if st.listed && st.entries.is_empty() {
        return ShellEmptyState::create(s("Your library is empty"))
            .with_icon(s("local_library"))
            .with_detail(s(
                "Add an EPUB book, a text or an HTML file. Each book is kept in your data folder.",
            ))
            .with_action_label(s("Add a Book"))
            .with_on_action(
                command(app, Command::AddBooks),
                on_command as ButtonOnClickCallbackType,
            )
            .dom()
            .with_id(ids::EMPTY);
    }
    let search = TextInput::create_search()
        .with_text(st.query.as_str())
        .with_placeholder("Search titles and authors")
        .with_accessibility_name("Search the library")
        .with_on_text_input(
            app.clone(),
            on_search_text as TextInputOnTextInputCallbackType,
        )
        .with_on_virtual_key_down(
            app.clone(),
            on_search_key as TextInputOnVirtualKeyDownCallbackType,
        )
        .dom()
        .with_id(ids::LIBRARY_SEARCH);
    let mut grid = Dom::create_div().with_id(ids::LIBRARY_GRID).with_css(
        "display: flex; flex-direction: row; flex-wrap: wrap; align-content: flex-start; \
         padding: 8px; flex-grow: 1; min-height: 0px; overflow-y: auto;",
    );
    let shown = library::shown(&st.entries, st.shelf, &st.query, st.sort);
    for (i, entry) in shown.iter().enumerate() {
        grid.add_child(tile(app, st, i, entry));
    }
    if shown.is_empty() {
        grid.add_child(
            Dom::create_div()
                .with_css("padding: 24px; color: system:secondary-text; font-size: 13px;")
                .with_child(Dom::create_span_with_text(if st.listed {
                    "No book here."
                } else {
                    "Reading the library..."
                })),
        );
    }
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; background: system:background;")
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; padding: 8px 14px 0px 14px;")
                .with_child(search),
        )
        .with_child(grid)
}

/// The status bar: how many books, the notice.
#[must_use]
pub fn status_bar(st: &AppState) -> Dom {
    let shown = library::shown(&st.entries, st.shelf, &st.query, st.sort).len();
    let mut segments = vec![StatusBarSegment::create(s(&format!(
        "{shown} OF {} BOOKS",
        st.entries.len()
    )))];
    if !st.notice.is_empty() {
        segments.push(StatusBarSegment::create(s(&st.notice)));
    }
    StatusBar::create(segments).dom()
}

extern "C" fn on_search_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    if let Some(mut st) = data.downcast_mut::<AppState>() {
        st.query = state.get_text().as_str().to_string();
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// Enter filters the grid; Escape clears the search.
extern "C" fn on_search_key(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let keep = OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    };
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    let Some(mut st) = data.downcast_mut::<AppState>() else {
        return keep;
    };
    match key {
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) => {
            st.query = state.get_text().as_str().trim().to_string();
            OnTextInputReturn {
                update: Update::RefreshDom,
                valid: TextInputValid::Yes,
            }
        }
        Some(VirtualKeyCode::Escape) => {
            st.query.clear();
            OnTextInputReturn {
                update: Update::RefreshDom,
                valid: TextInputValid::Yes,
            }
        }
        _ => keep,
    }
}
