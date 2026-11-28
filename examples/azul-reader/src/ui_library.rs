//! The library: the thin toolbar (Add a book, the search, the sort, Settings), the shelves as a
//! row of choices, the covers grid, the line of notices. A tile is the book's cover (or a
//! coloured tile with its title when it has none), its title, its author and how far the reader
//! is; a tap opens the book, the bin under the tile removes it.
//!
//! TODO(WIDGETS9A): IconGrid - the grid here is a wrapping row of tiles (no virtualization,
//! no keyboard navigation, no rubber band); WIDGETS9A's IconGrid replaces it.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CallbackInfo, RefAny, SegmentedOnChangeCallbackType, Update,
    },
    css::{EventFilter, HoverEventFilter},
    dom::{AccessibilityInfo, AccessibilityRole, Dom, VirtualKeyCode},
    shells::ShellEmptyState,
    str::String as AzString,
    vec::StringVec,
    widgets::{
        Button, OnTextInputReturn, Segmented, SegmentedState, TextInputState, TextInputValid,
    },
};

use azul_appkit::l10n::{label, t, t_args, Arg};

use crate::{
    app::{command, AppState, Command},
    commands::{on_command, run},
    ids,
    library::{self, LibraryEntry, Shelf},
    position, toolbar,
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

/// The shelves as a row of choices, each with how many books it holds.
fn shelves(app: &RefAny, st: &AppState) -> Dom {
    let names: Vec<String> = Shelf::ALL
        .iter()
        .map(|shelf| {
            let count = st.entries.iter().filter(|e| shelf.holds(e)).count();
            format!("{} {count}", t(shelf.label()))
        })
        .collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let selected = Shelf::ALL.iter().position(|shelf| *shelf == st.shelf).unwrap_or(0);
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; padding: 8px 12px 0px 12px; flex-shrink: 0; overflow-x: auto;")
        .with_child(
            Segmented::create(StringVec::from_vec(refs.iter().map(|n| s(n)).collect()))
                .with_selected_index(selected)
                .with_on_change(app.clone(), on_shelf as SegmentedOnChangeCallbackType)
                .dom()
                .with_id(ids::SHELF_ROW)
                .with_accessibility_assign(AccessibilityInfo::named(
                    label("azreader-shelves"),
                    AccessibilityRole::Grouping,
                )),
        )
}

extern "C" fn on_shelf(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let shelf = Shelf::ALL[state.selected_index.min(Shelf::ALL.len() - 1)];
    run(&mut data, Command::Shelf(shelf), &mut info)
}

/// One book's tile.
fn tile(app: &RefAny, st: &AppState, index: usize, entry: &LibraryEntry) -> Dom {
    let id = entry.info.id.clone();
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
        t("azreader-badge-finished")
    } else if entry.state.last_read == 0 {
        t("azreader-badge-new")
    } else {
        position::percent_label(entry.state.progress)
    };
    // The cover, the title, the author open the book; the bin under them removes it.
    let mut opens = Dom::create_div()
        .with_css("display: flex; flex-direction: column; cursor: pointer;")
        .with_accessibility_info(AccessibilityInfo::named(entry.info.title.as_str(), AccessibilityRole::PushButton))
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
        );
    opens.add_callback(
        EventFilter::Hover(HoverEventFilter::Click),
        command(app, Command::OpenBook(id.clone())),
        on_command,
    );
    let remove = Button::create("")
        .with_icon("delete")
        .with_on_click(command(app, Command::RemoveBook(id)), on_command as ButtonOnClickCallbackType)
        .dom()
        .with_accessibility_assign(AccessibilityInfo::named(
            t_args("azreader-remove-named", &[("name", Arg::from(entry.info.title.as_str()))]),
            AccessibilityRole::PushButton,
        ));
    Dom::create_div()
        .with_id(ids::indexed(&ids::BOOK_TILE, index))
        .with_class(ids::BOOK_TILE_CLASS)
        .with_css("display: flex; flex-direction: column; width: 136px; padding: 8px; margin: 6px; border-radius: 4px;")
        .with_child(opens)
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center; justify-content: space-between;")
                .with_child(
                    Dom::create_div()
                        .with_css("font-size: 11px; color: system:tertiary-text;")
                        .with_child(Dom::create_span_with_text(progress)),
                )
                .with_child(remove),
        )
}

/// The covers grid (or the empty state), under the shelves.
#[must_use]
pub fn content(app: &RefAny, st: &AppState) -> Dom {
    if st.listed && st.entries.is_empty() {
        return ShellEmptyState::create(t("azreader-library-empty"))
            .with_icon(s("local_library"))
            .with_detail(t("azreader-library-empty-detail"))
            .with_action_label(t("azreader-add-book"))
            .with_on_action(
                command(app, Command::AddBooks),
                on_command as ButtonOnClickCallbackType,
            )
            .dom()
            .with_id(ids::EMPTY);
    }
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
                .with_child(Dom::create_span_with_text(t(if st.listed {
                    "azreader-no-book-here"
                } else {
                    "azreader-reading-library"
                }))),
        );
    }
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; background: system:background;")
        .with_child(shelves(app, st))
        .with_child(grid)
}

/// The library's screen: the toolbar over the shelves and the covers, the notice under them.
#[must_use]
pub fn screen(app: &RefAny, st: &AppState) -> Dom {
    let mut screen = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(toolbar::library(app, st))
        .with_child(content(app, st));
    if !st.notice.is_empty() {
        screen.add_child(
            Dom::create_div()
                .with_id(ids::NOTICE)
                .with_css("padding: 6px 12px; font-size: 12px; flex-shrink: 0; border-top: 1px solid system:separator; color: system:secondary-text;")
                .with_child(Dom::create_span_with_text(st.notice.as_str())),
        );
    }
    screen
}

pub extern "C" fn on_search_text(
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
pub extern "C" fn on_search_key(
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
