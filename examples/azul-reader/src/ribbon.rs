//! The ribbons. The library's: HOME (Add a book, Open, Remove; Sort by), VIEW (Settings,
//! About). The reader's: READ (Library, Contents, Bookmarks, Bookmark this page, Previous /
//! Next page, Book start), VIEW (Text size, Line spacing, Margins, Paper, Font, Pages, Justify,
//! Settings). Every button runs a [`Command`].

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CallbackInfo, RefAny, RibbonOnTabClickCallbackType, Update,
    },
    dom::Dom,
    str::String as AzString,
    widgets::{Ribbon, RibbonGroup, RibbonTab},
};
use azul_appkit::ribbon::{column, group, large, large_toggle, small, toggle, RibbonCommand};

use crate::{
    app::{command, AppState, Command, Pane, Screen, LIBRARY_TABS, READER_TABS},
    commands::on_command,
    library::Sort,
    settings::{FontChoice, PageLayout, Paper},
};

/// A tab's name: a key said in the window's language.
fn s(text: &str) -> AzString {
    azul_appkit::l10n::label(text)
}

/// Every ribbon button runs a [`Command`] through [`on_command`] (azul-appkit's ribbon
/// builder).
impl RibbonCommand for Command {
    fn click_data(self, app: &RefAny) -> RefAny {
        command(app, self)
    }

    fn on_click() -> ButtonOnClickCallbackType {
        on_command
    }
}

extern "C" fn on_tab_click(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    crate::commands::run(&mut data, Command::RibbonTab(index), &mut info)
}

fn window_group(app: &RefAny) -> RibbonGroup {
    group(
        "azreader-group-window",
        vec![
            large(app, "settings", "azreader-settings", Command::Settings),
            large(app, "info", "azreader-about", Command::About),
        ],
    )
}

fn library_home(app: &RefAny, st: &AppState) -> RibbonTab {
    let selected = st.selected.clone();
    let mut books = vec![large(
        app,
        "library_add",
        "azreader-add-book",
        Command::AddBooks,
    )];
    if let Some(id) = selected {
        books.push(large(
            app,
            "menu_book",
            "azreader-open",
            Command::OpenBook(id.clone()),
        ));
        books.push(large(
            app,
            "delete",
            "azreader-remove",
            Command::RemoveBook(id),
        ));
    }
    if st.open.is_some() {
        books.push(large(
            app,
            "auto_stories",
            "azreader-back-to-book",
            Command::ShowReader,
        ));
    }
    let sort = group(
        "azreader-group-sort",
        vec![column(
            Sort::ALL
                .iter()
                .map(|sort| {
                    toggle(
                        app,
                        "sort",
                        sort.label(),
                        Command::Sort(*sort),
                        st.sort == *sort,
                    )
                })
                .collect(),
        )],
    );
    RibbonTab::create(s(LIBRARY_TABS[0]))
        .with_group(group("azreader-group-books", books))
        .with_group(sort)
}

fn reader_read(app: &RefAny, st: &AppState) -> RibbonTab {
    let marked = st
        .open
        .as_ref()
        .zip(st.position())
        .is_some_and(|(open, at)| open.state.bookmark_at(at).is_some());
    let go = group(
        "azreader-group-go",
        vec![
            large(
                app,
                "local_library",
                "azreader-library",
                Command::ShowLibrary,
            ),
            large_toggle(
                app,
                "toc",
                "azreader-contents",
                Command::Pane(Pane::Contents),
                st.pane == Pane::Contents,
            ),
            large_toggle(
                app,
                "bookmarks",
                "azreader-bookmarks",
                Command::Pane(Pane::Bookmarks),
                st.pane == Pane::Bookmarks,
            ),
        ],
    );
    let mark = group(
        "azreader-group-bookmark",
        vec![large_toggle(
            app,
            if marked {
                "bookmark_remove"
            } else {
                "bookmark_add"
            },
            if marked {
                "azreader-remove-bookmark"
            } else {
                "azreader-bookmark-page"
            },
            Command::ToggleBookmark,
            marked,
        )],
    );
    let pages = group(
        "azreader-group-pages",
        vec![column(vec![
            small(
                app,
                "navigate_before",
                "azreader-previous-page",
                Command::PrevPage,
            ),
            small(
                app,
                "navigate_next",
                "azreader-next-page",
                Command::NextPage,
            ),
            small(app, "first_page", "azreader-book-start", Command::BookStart),
        ])],
    );
    RibbonTab::create(s(READER_TABS[0]))
        .with_group(go)
        .with_group(mark)
        .with_group(pages)
}

fn reader_view(app: &RefAny, st: &AppState) -> RibbonTab {
    let type_group = group(
        "azreader-group-text",
        vec![
            column(vec![
                small(
                    app,
                    "text_increase",
                    "azreader-larger",
                    Command::FontSize(2),
                ),
                small(
                    app,
                    "text_decrease",
                    "azreader-smaller",
                    Command::FontSize(-2),
                ),
                toggle(
                    app,
                    "format_align_justify",
                    "azreader-justify",
                    Command::Justify(!st.settings.justify),
                    st.settings.justify,
                ),
            ]),
            column(
                FontChoice::ALL
                    .iter()
                    .map(|f| {
                        toggle(
                            app,
                            "font_download",
                            f.label(),
                            Command::Font(*f),
                            st.settings.font == *f,
                        )
                    })
                    .collect(),
            ),
        ],
    );
    let spacing = group(
        "azreader-group-spacing",
        vec![
            column(vec![
                small(
                    app,
                    "format_line_spacing",
                    "azreader-more-line-space",
                    Command::LineSpacing(1),
                ),
                small(
                    app,
                    "density_small",
                    "azreader-less-line-space",
                    Command::LineSpacing(-1),
                ),
            ]),
            column(vec![
                small(
                    app,
                    "width_wide",
                    "azreader-wider-margins",
                    Command::Margins(8),
                ),
                small(
                    app,
                    "width_normal",
                    "azreader-narrower-margins",
                    Command::Margins(-8),
                ),
            ]),
        ],
    );
    let paper = group(
        "azreader-group-paper",
        vec![column(
            Paper::ALL
                .iter()
                .map(|p| {
                    toggle(
                        app,
                        "palette",
                        p.label(),
                        Command::Paper(*p),
                        st.settings.paper == *p,
                    )
                })
                .collect(),
        )],
    );
    let layout = group(
        "azreader-group-layout",
        vec![column(
            PageLayout::ALL
                .iter()
                .map(|l| {
                    toggle(
                        app,
                        "auto_stories",
                        l.label(),
                        Command::Layout(*l),
                        st.settings.layout == *l,
                    )
                })
                .collect(),
        )],
    );
    RibbonTab::create(s(READER_TABS[1]))
        .with_group(type_group)
        .with_group(spacing)
        .with_group(paper)
        .with_group(layout)
        .with_group(window_group(app))
}

/// The ribbon for the screen on show.
#[must_use]
pub fn ribbon(app: &RefAny, st: &AppState) -> Dom {
    let tabs = match st.screen {
        Screen::Library => vec![
            library_home(app, st),
            RibbonTab::create(s(LIBRARY_TABS[1])).with_group(window_group(app)),
        ],
        Screen::Reader => vec![reader_read(app, st), reader_view(app, st)],
    };
    let active = st.ribbon_tab.min(tabs.len() - 1);
    Ribbon::create(tabs)
        .with_active_tab(active)
        .with_on_tab_click(app.clone(), on_tab_click as RibbonOnTabClickCallbackType)
        // No title row over the ribbon: its tabs are the title bar.
        .with_tabs_in_titlebar(azul_appkit::ui::tabs_in_titlebar())
        .dom_desktop()
}
