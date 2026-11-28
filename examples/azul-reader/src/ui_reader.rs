//! The reader: the thin toolbar, the strip beside the pages (a desktop), the pages on a quiet
//! ground, and - on a phone - the page over the whole screen with the toolbar and a slider that
//! a tap in the middle of the page shows and hides.
//!
//! A page on screen is the chapter's reading column ([`paginate::column_dom`], the box the
//! pagination laid out) seen through a clip window: the paper (the page's margins and colours)
//! holds a box of the page's text width and of the page's own height (its span: from its start
//! to the next page's start) with `overflow: hidden`, and in it the column moved up by the
//! page's start (`position: relative; top: -start`). The engine's slicer cuts its PDF pages
//! the same way - content never moves, it is only clipped - so a page shows exactly the lines
//! the pagination put on it. A page drawn ahead ([`crate::pagecache`]) is the same clip window
//! over its picture instead of the column; the column is laid out only for a page not drawn yet.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CallbackInfo, RefAny, SliderOnValueChangeCallbackType, Update,
    },
    css::{Css, EventFilter, HoverEventFilter},
    dom::{AccessibilityInfo, AccessibilityRole, Dom},
    option::OptionString,
    str::String as AzString,
    widgets::{Button, ButtonType, Slider, SliderState},
};

use azul_appkit::l10n::{label, t, t_args, Arg};

use crate::{
    app::{command, AppState, Command, Pane},
    chrome::{self, Tap},
    commands::{on_command, on_wheel, run},
    ids,
    pagecache::PageRun,
    paginate,
    position::{self, PageMap},
    settings::{PageGeometry, PaperColors},
    strip, toolbar,
};

fn text(content: &str, css: &str) -> Dom {
    Dom::create_div()
        .with_css(css.to_string())
        .with_child(Dom::create_span_with_text(content))
}

/// The views' slides: a new view comes in from below or above with both its pages, the one it
/// replaces leaves the other way (an exit is resolved in the old tree's sheet, an entrance in
/// the new one's: the reading area carries them on every build).
const KEYFRAMES: &str = "\
@keyframes azr-in-below { from { transform: translateY(100%); } to { transform: translateY(0%); } }
@keyframes azr-in-above { from { transform: translateY(-100%); } to { transform: translateY(0%); } }
@keyframes azr-out-above { from { transform: translateY(0%); } to { transform: translateY(-100%); } }
@keyframes azr-out-below { from { transform: translateY(0%); } to { transform: translateY(100%); } }
@keyframes azr-fade-out { from { opacity: 1; } to { opacity: 0; } }
";

/// One page: the paper, and on it the clip window over the reading column.
fn page_frame(
    column: Dom,
    span: (f32, f32),
    geometry: &PageGeometry,
    colors: &PaperColors,
    index: usize,
    gutter: f32,
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
    paper(clip, geometry, colors, index, gutter)
}

/// One page drawn ahead ([`crate::pagecache`]): the paper, and on it a clip window over the
/// picture of its run, moved up by the page's place in the run (`k` pages down).
fn page_picture(
    run: &PageRun,
    k: usize,
    geometry: &PageGeometry,
    colors: &PaperColors,
    index: usize,
    gutter: f32,
) -> Dom {
    let width = geometry.text_width;
    let height = geometry.text_height;
    #[allow(clippy::cast_precision_loss)]
    let (top, total) = (k as f32 * height, run.count as f32 * height);
    let picture = Dom::create_image(run.image.clone()).with_css(format!(
        "display: block; position: relative; top: -{top}px; width: {width}px; height: {total}px; \
         flex-shrink: 0;"
    ));
    let clip = Dom::create_div()
        .with_class(ids::PAGE_CLIP_CLASS)
        .with_css(format!(
            "display: block; position: relative; overflow: hidden; width: {width}px; height: \
             {height}px; flex-shrink: 0;"
        ))
        .with_child(picture);
    paper(clip, geometry, colors, index, gutter)
}

/// A page's paper (its margins and colours) around `content`.
fn paper(content: Dom, geometry: &PageGeometry, colors: &PaperColors, index: usize, gutter: f32) -> Dom {
    Dom::create_div()
        .with_id(ids::indexed(&ids::PAGE, index))
        .with_class(ids::PAGE_CLASS)
        .with_css(format!(
            "display: flex; flex-direction: column; flex-shrink: 0; box-sizing: content-box; \
             width: {}px; height: {}px; padding: {}px; margin: 0px {gutter}px; background: {}; \
             color: {}; overflow: hidden; border: 1px solid system:separator;",
            geometry.text_width, geometry.text_height, geometry.margin, colors.background, colors.text
        ))
        .with_child(content)
}

/// An empty paper (the right page of a spread after the chapter's last page).
fn blank_page(geometry: &PageGeometry, colors: &PaperColors, index: usize, gutter: f32) -> Dom {
    Dom::create_div()
        .with_id(ids::indexed(&ids::PAGE, index))
        .with_class(ids::PAGE_CLASS)
        .with_css(format!(
            "display: block; flex-shrink: 0; box-sizing: content-box; width: {}px; height: {}px; \
         padding: {}px; margin: 0px {gutter}px; background: {}; border: 1px solid system:separator;",
            geometry.text_width, geometry.text_height, geometry.margin, colors.background
        ))
}

/// A desktop's arrow beside the pages that turns the page.
fn turn_zone(app: &RefAny, id: AzString, glyph: &str, cmd: Command) -> Dom {
    let mut zone = Dom::create_div()
        .with_id(id)
        .with_css(format!(
            "display: flex; flex-direction: column; justify-content: center; align-items: center; \
             width: {}px; flex-shrink: 0; align-self: stretch; font-size: 28px; \
             color: system:tertiary-text; cursor: pointer;",
            chrome::ARROW_PX
        ))
        .with_accessibility_name(t(if matches!(cmd, Command::PrevPage) {
            "azreader-previous-page"
        } else {
            "azreader-next-page"
        }))
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
    // The view's own node (its id names its first page): a turn brings a new one, which slides
    // in from below (forward) or from above (back) with both its pages, while the one it
    // replaces leaves the same way - the way of the turn before it, which is the way the reader
    // keeps going.
    let chapter = st.chapter.as_ref().map_or(0, |c| c.chapter);
    let motion = match st.turned {
        Some(true) => {
            "-azul-animation-in: azr-in-below 300ms ease-out; -azul-animation-out: azr-out-above \
             300ms ease-out;"
        }
        Some(false) => {
            "-azul-animation-in: azr-in-above 300ms ease-out; -azul-animation-out: azr-out-below \
             300ms ease-out;"
        }
        None => "-azul-animation-out: azr-fade-out 200ms ease-in;",
    };
    let gutter = if st.phone { 0.0 } else { chrome::GUTTER_PX };
    let mut row = Dom::create_div()
        .with_id(AzString::from(format!("{}-{chapter}-{}", ids::SPREAD_PREFIX, st.page)))
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: flex-start; {motion}"
        ));
    let drawn = st
        .pictures_key()
        .is_some_and(|key| key == st.pictures.key);
    for i in 0..geometry.per_view {
        let page = st.page + i;
        let picture = st.pictures.find(page).filter(|_| drawn);
        if let Some((run, k)) = picture {
            row.add_child(page_picture(run, k, geometry, colors, i, gutter));
        } else if page < pages.page_count() {
            let column = paginate::column_dom(xml, &st.settings, geometry.text_width, st.flora)
                .with_class(ids::COLUMN_CLASS);
            row.add_child(page_frame(column, pages.span(page), geometry, colors, i, gutter));
        } else {
            row.add_child(blank_page(geometry, colors, i, gutter));
        }
    }
    row
}

/// The reading area: the running head, the pages (between the arrows of a desktop), the folio.
/// On a phone the area takes three zones over it (a tap on a side turns the page, a tap in the
/// middle shows or hides the toolbar and the slider).
#[must_use]
pub fn pages(app: &RefAny, st: &AppState) -> Dom {
    let geometry = st.geometry();
    let colors = st.settings.paper.colors(st.dark, st.flora);
    let mut area = Dom::create_div()
        .with_id(ids::READING_AREA)
        .with_marker(OptionString::Some(st.area_marker.clone()))
        .with_component_css(Css::from_string(KEYFRAMES))
        .with_css(
            "position: relative; display: flex; flex-direction: column; align-items: center; \
             justify-content: center; flex-grow: 1; min-height: 0px; min-width: 0px; \
             overflow: hidden; background: system:under-page-background;",
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
        .with_css("display: flex; flex-direction: row; align-items: center; justify-content: center;");
    if !st.phone {
        row.add_child(turn_zone(app, ids::PREV_ZONE, "\u{2039}", Command::PrevPage));
    }
    match chapter {
        Some(c) => row.add_child(spread(st, &c.pages, &c.content.xml, &geometry, &colors)),
        None => row.add_child(
            Dom::create_div()
                .with_id(ids::LOADING)
                .with_css(format!(
                    "display: flex; flex-direction: column; justify-content: center; align-items: center; \
                     width: {}px; height: {}px; padding: {}px; background: {}; color: {};",
                    geometry.text_width, geometry.text_height, geometry.margin, colors.background, colors.muted
                ))
                .with_child(Dom::create_span_with_text(t("azreader-laying-out-pages"))),
        ),
    }
    if !st.phone {
        row.add_child(turn_zone(app, ids::NEXT_ZONE, "\u{203a}", Command::NextPage));
    }
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
    if st.phone {
        area.add_child(tap_zones(app));
    }
    area.add_callback(
        EventFilter::Hover(HoverEventFilter::Scroll),
        app.clone(),
        on_wheel,
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

/// The three zones over a phone's page: the left third turns back, the right third forward, the
/// middle shows or hides the toolbar and the slider. They sit over the text (a reader has no
/// selection to make), each named for a screen reader.
fn tap_zones(app: &RefAny) -> Dom {
    let zone = |id: AzString, left: &str, width: &str, tap: Tap| {
        let (cmd, name) = match tap {
            Tap::Previous => (Command::PrevPage, "azreader-previous-page"),
            Tap::Next => (Command::NextPage, "azreader-next-page"),
            Tap::Toggle => (Command::ToggleChrome, "azreader-toggle-controls"),
        };
        let mut zone = Dom::create_div()
            .with_id(id)
            .with_css(format!(
                "position: absolute; top: 0px; bottom: 0px; left: {left}; width: {width};"
            ))
            .with_accessibility_info(AccessibilityInfo::named(label(name), AccessibilityRole::PushButton));
        zone.add_callback(EventFilter::Hover(HoverEventFilter::Click), command(app, cmd), on_command);
        zone
    };
    let side = format!("{}%", (chrome::SIDE_SHARE * 100.0).round());
    let middle = format!("{}%", (100.0 - 2.0 * chrome::SIDE_SHARE * 100.0).round());
    Dom::create_div()
        .with_css("position: absolute; top: 0px; bottom: 0px; left: 0px; right: 0px;")
        .with_child(zone(ids::ZONE_LEFT, "0%", &side, Tap::Previous))
        .with_child(zone(ids::ZONE_MIDDLE, &side, &middle, Tap::Toggle))
        .with_child(zone(ids::ZONE_RIGHT, &format!("{}%", (100.0 - chrome::SIDE_SHARE * 100.0).round()), &side, Tap::Next))
}

/// The slider row under a phone's page: the contents button, the slider through the book, where
/// the reader is.
fn slider_row(app: &RefAny, st: &AppState) -> Dom {
    let progress = st.progress();
    // While a jump waits for its chapter the thumb stays where the finger put it.
    let thumb = match (st.pending.is_some(), st.last_seek) {
        (true, Some(asked)) => asked as f32,
        _ => chrome::slider_value(progress),
    };
    let count = st.chapter.as_ref().map_or(0, |c| c.pages.page_count());
    let page = chrome::page_number(st.page, count);
    let place = t_args(
        "azreader-where",
        &[
            ("percent", Arg::from(position::percent_label(progress))),
            ("page", Arg::from(page)),
            ("count", Arg::from(count)),
        ],
    );
    // The sheet of pages, contents and bookmarks opens from here (a phone's toolbar has no room
    // for it); the same button closes it.
    let contents = Button::with_type(
        AzString::from(""),
        if st.pane == Pane::None { ButtonType::Default } else { ButtonType::Primary },
    )
    .with_icon("toc")
    .with_on_click(
        // Closed: the contents open. Open (on whatever view): the same view again closes it.
        command(app, Command::Pane(if st.pane == Pane::None { Pane::Contents } else { st.pane })),
        on_command as ButtonOnClickCallbackType,
    )
    .dom()
    .with_id(ids::SLIDER_CONTENTS)
    .with_accessibility_assign(AccessibilityInfo::named(label("azreader-strip"), AccessibilityRole::PushButton));
    let seek = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-width: 0px;")
        .with_child(
            Slider::create(thumb, 0.0, chrome::SLIDER_MAX)
                .with_accessibility_name(label("azreader-seek"))
                .with_on_value_change(app.clone(), on_slider as SliderOnValueChangeCallbackType)
                .dom()
                // The track is as wide as the row (its design width is 200 px): the scrubber of a book.
                .with_css("width: 100%;")
                .with_id(ids::SLIDER),
        );
    Dom::create_div()
        .with_id(ids::SLIDER_ROW)
        .with_css(format!(
            "display: flex; flex-direction: column; row-gap: 2px; padding: 4px 16px 8px 16px; \
             min-height: {}px; background: system:window-background; \
             border-top: 1px solid system:separator;",
            chrome::SLIDER_ROW_PX
        ))
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center; column-gap: 12px;")
                .with_child(contents)
                .with_child(seek),
        )
        .with_child(text(&place, "font-size: 12px; text-align: center; color: system:secondary-text;"))
}

/// The slider moved: the reader goes there.
extern "C" fn on_slider(mut data: RefAny, mut info: CallbackInfo, state: SliderState) -> Update {
    run(&mut data, Command::Seek(chrome::slider_progress(state.value)), &mut info)
}

/// The reader's screen: the toolbar over the strip and the pages (a desktop), the pages over the
/// whole screen with the toolbar and the slider floating over them while a tap has them shown (a
/// phone).
#[must_use]
pub fn screen(app: &RefAny, st: &AppState) -> Dom {
    let pages = pages(app, st);
    let strip = strip::strip(app, st);
    if st.phone {
        let mut stage = Dom::create_div()
            .with_css("position: relative; display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; min-width: 0px;")
            .with_child(pages);
        if st.chrome {
            stage.add_child(
                Dom::create_div()
                    .with_css("position: absolute; top: 0px; left: 0px; right: 0px;")
                    .with_child(toolbar::reader(app, st)),
            );
        }
        if let Some(strip) = strip {
            stage.add_child(strip);
        }
        let mut screen = Dom::create_div()
            .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
            .with_child(stage);
        if st.chrome {
            screen.add_child(slider_row(app, st));
        }
        return screen;
    }
    let mut body = Dom::create_div()
        .with_css("display: flex; flex-direction: row; flex-grow: 1; min-height: 0px; min-width: 0px;");
    if let Some(strip) = strip {
        body.add_child(strip);
    }
    body.add_child(pages);
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(toolbar::reader(app, st))
        .with_child(body)
}

#[cfg(test)]
#[path = "ui_reader_test.rs"]
mod ui_reader_test;
