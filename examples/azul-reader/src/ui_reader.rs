//! The reader: the side pane (the table of contents or the bookmarks), the pages, the status
//! bar.
//!
//! A page on screen is the chapter's reading column ([`paginate::column_dom`], the box the
//! pagination laid out) seen through a clip window: the paper (the page's margins and colours)
//! holds a box of the page's text width and of the page's own height (its span: from its start
//! to the next page's start) with `overflow: hidden`, and in it the column moved up by the
//! page's start (`position: relative; top: -start`). The engine's slicer cuts its PDF pages
//! the same way - content never moves, it is only clipped - so a page shows exactly the lines
//! the pagination put on it.

use azul::{
    callbacks::{ButtonOnClickCallbackType, RefAny},
    css::{EventFilter, HoverEventFilter},
    dom::Dom,
    option::OptionString,
    str::String as AzString,
    widgets::{Button, StatusBar, StatusBarSegment},
};

use crate::{
    app::{command, AppState, Command, Pane},
    commands::on_command,
    ids, paginate,
    position::{self, PageMap},
    settings::{PageGeometry, PaperColors},
};

fn s(text: &str) -> AzString {
    AzString::from(text)
}

fn text(content: &str, css: &str) -> Dom {
    Dom::create_div()
        .with_css(css.to_string())
        .with_child(Dom::create_span_with_text(content))
}

const PANE_CSS: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; \
                        overflow-y: auto; padding: 6px 0px; background: system:window-background;";
const PANE_TITLE_CSS: &str = "padding: 6px 14px; font-size: 12px; font-weight: 600; \
                              color: system:secondary-text;";

/// The side pane (`None` when it is closed).
#[must_use]
pub fn navigation(app: &RefAny, st: &AppState) -> Option<Dom> {
    let open = st.open.as_ref()?;
    match st.pane {
        Pane::None => None,
        Pane::Contents => {
            let current = st.chapter.as_ref().map(|c| c.chapter);
            let current_entry =
                current.and_then(|c| open.book.toc.iter().position(|e| e.chapter == Some(c)));
            let mut pane = Dom::create_div()
                .with_id(ids::TOC)
                .with_css(PANE_CSS)
                .with_child(text("CONTENTS", PANE_TITLE_CSS));
            for (i, entry) in open.book.toc.iter().enumerate() {
                let on = current_entry == Some(i);
                let mut row = Dom::create_div()
                    .with_id(ids::indexed(&ids::TOC_ROW, i))
                    .with_class(ids::TOC_ROW_CLASS)
                    .with_css(format!(
                        "padding: 5px 14px 5px {}px; font-size: 13px; {} {}",
                        14 + 14 * entry.depth.min(6),
                        if entry.chapter.is_some() {
                            "color: system:text; cursor: pointer;"
                        } else {
                            "color: system:secondary-text;"
                        },
                        if on {
                            "background: system:selection-background; font-weight: 600;"
                        } else {
                            ""
                        }
                    ))
                    .with_accessibility_name(entry.label.as_str())
                    .with_child(Dom::create_span_with_text(entry.label.as_str()));
                if on {
                    row.add_class(ids::TOC_CURRENT_CLASS);
                }
                if entry.chapter.is_some() {
                    row.add_callback(
                        EventFilter::Hover(HoverEventFilter::Click),
                        command(app, Command::GoToEntry(i)),
                        on_command,
                    );
                }
                pane.add_child(row);
            }
            Some(pane)
        }
        Pane::Bookmarks => {
            let mut pane = Dom::create_div()
                .with_id(ids::BOOKMARKS)
                .with_css(PANE_CSS)
                .with_child(text("BOOKMARKS", PANE_TITLE_CSS));
            if open.state.bookmarks.is_empty() {
                pane.add_child(text(
                    "No bookmark yet. Bookmark Page (Ctrl/Cmd+D) keeps the page you read.",
                    "padding: 6px 14px; font-size: 12px; color: system:secondary-text;",
                ));
            }
            for (i, mark) in open.state.bookmarks.iter().enumerate() {
                let percent =
                    position::percent_label(position::book_progress(&open.weights, mark.position));
                let mut body = Dom::create_div()
                    .with_css(
                        "display: flex; flex-direction: column; flex-grow: 1; cursor: pointer;",
                    )
                    .with_child(text(
                        &format!("{} - {percent}", mark.chapter_title),
                        "font-size: 11px; color: system:secondary-text;",
                    ))
                    .with_child(text(&mark.label, "font-size: 13px; color: system:text;"));
                body.add_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    command(app, Command::GoToBookmark(i)),
                    on_command,
                );
                let remove = Button::create("")
                    .with_icon("close")
                    .with_on_click(
                        command(app, Command::RemoveBookmark(i)),
                        on_command as ButtonOnClickCallbackType,
                    )
                    .dom();
                pane.add_child(
                    Dom::create_div()
                        .with_id(ids::indexed(&ids::BOOKMARK_ROW, i))
                        .with_class(ids::BOOKMARK_ROW_CLASS)
                        .with_css("display: flex; flex-direction: row; align-items: center; padding: 6px 8px 6px 14px;")
                        .with_child(body)
                        .with_child(remove),
                );
            }
            Some(pane)
        }
    }
}

/// One page: the paper, and on it the clip window over the reading column.
fn page_frame(
    column: Dom,
    span: (f32, f32),
    geometry: &PageGeometry,
    colors: &PaperColors,
    index: usize,
) -> Dom {
    let (top, height) = span;
    let width = geometry.text_width;
    let shift = Dom::create_div()
        .with_css(format!(
            "display: block; position: relative; top: -{top}px; width: {width}px;"
        ))
        .with_child(column);
    let clip = Dom::create_div()
        .with_class(ids::PAGE_CLIP_CLASS)
        .with_css(format!(
            "display: block; position: relative; overflow: hidden; width: {width}px; height: {height}px; \
             flex-shrink: 0;"
        ))
        .with_child(shift);
    Dom::create_div()
        .with_id(ids::indexed(&ids::PAGE, index))
        .with_class(ids::PAGE_CLASS)
        .with_css(format!(
            "display: flex; flex-direction: column; flex-shrink: 0; box-sizing: content-box; \
             width: {width}px; height: {}px; padding: {}px; margin: 0px 12px; background: {}; \
             color: {}; overflow: hidden; border: 1px solid system:separator;",
            geometry.text_height, geometry.margin, colors.background, colors.text
        ))
        .with_child(clip)
}

/// An empty paper (the right page of a spread after the chapter's last page).
fn blank_page(geometry: &PageGeometry, colors: &PaperColors, index: usize) -> Dom {
    Dom::create_div()
        .with_id(ids::indexed(&ids::PAGE, index))
        .with_class(ids::PAGE_CLASS)
        .with_css(format!(
            "display: block; flex-shrink: 0; box-sizing: content-box; width: {}px; height: {}px; \
         padding: {}px; margin: 0px 12px; background: {}; border: 1px solid system:separator;",
            geometry.text_width, geometry.text_height, geometry.margin, colors.background
        ))
}

/// A side zone that turns the page.
fn turn_zone(app: &RefAny, id: AzString, glyph: &str, cmd: Command) -> Dom {
    let mut zone = Dom::create_div()
        .with_id(id)
        .with_css(
            "display: flex; flex-direction: column; justify-content: center; align-items: center; \
             width: 36px; flex-shrink: 0; align-self: stretch; font-size: 28px; \
             color: system:tertiary-text; cursor: pointer;",
        )
        .with_accessibility_name(if matches!(cmd, Command::PrevPage) {
            "Previous page"
        } else {
            "Next page"
        })
        .with_child(Dom::create_span_with_text(glyph));
    zone.add_callback(
        EventFilter::Hover(HoverEventFilter::Click),
        command(app, cmd),
        on_command,
    );
    zone
}

/// The pages of the view.
fn spread(
    st: &AppState,
    pages: &PageMap,
    xml: &azul::xml::Xml,
    geometry: &PageGeometry,
    colors: &PaperColors,
) -> Dom {
    let mut row =
        Dom::create_div().with_css("display: flex; flex-direction: row; align-items: flex-start;");
    for i in 0..geometry.per_view {
        let page = st.page + i;
        if page < pages.page_count() {
            let column = paginate::column_dom(xml, &st.settings, geometry.text_width)
                .with_class(ids::COLUMN_CLASS);
            row.add_child(page_frame(column, pages.span(page), geometry, colors, i));
        } else {
            row.add_child(blank_page(geometry, colors, i));
        }
    }
    row
}

/// The reading area: the running head, the pages between the turn zones, the folio.
#[must_use]
pub fn pages(app: &RefAny, st: &AppState) -> Dom {
    let geometry = st.geometry();
    let colors = st.settings.paper.colors(st.dark);
    let mut area = Dom::create_div()
        .with_id(ids::READING_AREA)
        .with_marker(OptionString::Some(st.area_marker.clone()))
        .with_css(
            "display: flex; flex-direction: column; align-items: center; justify-content: center; \
             flex-grow: 1; min-height: 0px; min-width: 0px; overflow: hidden; \
             background: system:under-page-background;",
        );
    let open = st.open.as_ref();
    let chapter = st
        .chapter
        .as_ref()
        .filter(|c| open.is_some_and(|o| o.info.id == c.book_id));
    let head = match (open, chapter) {
        (Some(o), Some(c)) => o.book.chapter_title(c.chapter),
        (Some(o), None) => o.info.title.clone(),
        _ => String::new(),
    };
    area.add_child(
        text(
            &head,
            "height: 20px; padding-bottom: 4px; font-size: 12px; color: system:secondary-text;",
        )
        .with_id(ids::RUNNING_HEAD),
    );
    let mut row = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; align-items: center; justify-content: center;",
        )
        .with_child(turn_zone(
            app,
            ids::PREV_ZONE,
            "\u{2039}",
            Command::PrevPage,
        ));
    match chapter {
        Some(c) => row.add_child(spread(st, &c.pages, &c.content.xml, &geometry, &colors)),
        None => row.add_child(
            Dom::create_div()
                .with_id(ids::LOADING)
                .with_css(format!(
                    "display: flex; flex-direction: column; justify-content: center; align-items: center; \
                     width: {}px; height: {}px; padding: {}px; margin: 0px 12px; background: {}; color: {};",
                    geometry.text_width, geometry.text_height, geometry.margin, colors.background, colors.muted
                ))
                .with_child(Dom::create_span_with_text("Laying out the pages\u{2026}")),
        ),
    }
    row.add_child(turn_zone(
        app,
        ids::NEXT_ZONE,
        "\u{203a}",
        Command::NextPage,
    ));
    area.add_child(row);
    let folio = match chapter {
        Some(c) => {
            let count = c.pages.page_count();
            let last = (st.page + geometry.per_view).min(count);
            if geometry.per_view > 1 && last > st.page + 1 {
                format!("{}-{} / {count}", st.page + 1, last)
            } else {
                format!("{} / {count}", st.page + 1)
            }
        }
        None => String::new(),
    };
    area.add_child(
        text(
            &folio,
            "height: 20px; padding-top: 4px; font-size: 12px; color: system:secondary-text;",
        )
        .with_id(ids::FOLIO),
    );
    area.add_callback(
        EventFilter::Hover(HoverEventFilter::SwipeLeft),
        command(app, Command::NextPage),
        on_command,
    );
    area.add_callback(
        EventFilter::Hover(HoverEventFilter::SwipeRight),
        command(app, Command::PrevPage),
        on_command,
    );
    area
}

/// The status bar: the chapter, its page, the book's progress, the notice.
#[must_use]
pub fn status_bar(st: &AppState) -> Dom {
    let mut segments = Vec::new();
    if let (Some(open), Some(c)) = (st.open.as_ref(), st.chapter.as_ref()) {
        segments.push(StatusBarSegment::create(s(&format!(
            "CHAPTER {} OF {}",
            c.chapter + 1,
            open.book.spine.len()
        ))));
        segments.push(StatusBarSegment::create(s(&format!(
            "PAGE {} OF {}",
            st.page + 1,
            c.pages.page_count()
        ))));
    }
    segments.push(
        StatusBarSegment::create(s(&format!(
            "{} READ",
            position::percent_label(st.progress())
        )))
        .with_marker(ids::PROGRESS),
    );
    if st.pending.is_some() {
        segments.push(StatusBarSegment::create(s("LAYING OUT")).with_icon(s("hourglass_empty")));
    }
    if !st.notice.is_empty() {
        segments.push(StatusBarSegment::create(s(&st.notice)));
    }
    StatusBar::create(segments).dom()
}
