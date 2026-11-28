//! The strip beside the pages, like the Chrome PDF viewer's sidebar: a row of three icon
//! buttons (the pages, the contents, the bookmarks) over what the one that is on lists. The
//! pages are thumbnails of the chapter on screen (the picture of a page where it is drawn,
//! else its paper with its number); the contents and the bookmarks are rows. A desktop keeps it
//! beside the pages (the toolbar's first button opens and closes it); on a phone it is a sheet
//! over the page's left side that a choice closes.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CallbackInfo, RefAny, ThumbnailStripOnEventCallbackType, Update,
    },
    css::{EventFilter, HoverEventFilter},
    dom::{AccessibilityInfo, AccessibilityRole, Dom},
    str::String as AzString,
    vec::ThumbnailItemVec,
    widgets::{
        Button, ButtonType, ThumbnailItem, ThumbnailStrip, ThumbnailStripEvent,
        ThumbnailStripEventKind,
    },
};
use azul_appkit::l10n::{label, t, t_args, Arg};

use crate::{
    app::{command, AppState, Command, Pane},
    chrome,
    commands::{on_command, run},
    ids, position,
    settings::{PageGeometry, PaperColors},
};

const PANE_CSS: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; \
                        overflow-y: auto; padding: 6px 0px; background: system:window-background;";
const PANE_TITLE_CSS: &str = "padding: 6px 14px; font-size: 12px; font-weight: 600; \
                              color: system:secondary-text;";

fn text(content: &str, css: &str) -> Dom {
    Dom::create_div()
        .with_css(css.to_string())
        .with_child(Dom::create_span_with_text(content))
}

/// An icon button of the strip's header, the one for the view on shown as the primary.
fn view_button(app: &RefAny, pane: Pane, on: bool, icon: &str, name: &str, id: AzString) -> Dom {
    Button::with_type(AzString::from(""), if on { ButtonType::Primary } else { ButtonType::Default })
        .with_icon(icon)
        .with_on_click(command(app, Command::Pane(pane)), on_command as ButtonOnClickCallbackType)
        .dom()
        .with_id(id)
        .with_accessibility_assign(AccessibilityInfo::named(label(name), AccessibilityRole::PushButton))
}

/// The strip (`None` when it is closed, or no book is open).
#[must_use]
pub fn strip(app: &RefAny, st: &AppState) -> Option<Dom> {
    st.open.as_ref()?;
    if st.pane == Pane::None {
        return None;
    }
    let header = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; column-gap: 4px; padding: 6px 8px; flex-shrink: 0;")
        .with_child(view_button(app, Pane::Pages, st.pane == Pane::Pages, "description", "azreader-thumbnails", ids::STRIP_PAGES))
        .with_child(view_button(app, Pane::Contents, st.pane == Pane::Contents, "toc", "azreader-contents", ids::STRIP_CONTENTS))
        .with_child(view_button(app, Pane::Bookmarks, st.pane == Pane::Bookmarks, "bookmarks", "azreader-bookmarks", ids::STRIP_BOOKMARKS));
    let body = match st.pane {
        Pane::Pages | Pane::None => pages(app, st),
        Pane::Contents => contents(app, st),
        Pane::Bookmarks => bookmarks(app, st),
    };
    let width = if st.phone {
        "width: 84%; max-width: 320px;".to_string()
    } else {
        format!("width: {}px;", chrome::STRIP_PX)
    };
    let sheet = Dom::create_div()
        .with_id(ids::STRIP)
        .with_css(format!(
            "display: flex; flex-direction: column; min-height: 0px; flex-shrink: 0; {width} \
             background: system:window-background; border-right: 1px solid system:separator;"
        ))
        .with_accessibility_info(AccessibilityInfo::named(label("azreader-strip"), AccessibilityRole::Pane))
        .with_child(header)
        .with_child(body);
    // A desktop's strip is a column beside the pages.
    if !st.phone {
        return Some(sheet);
    }
    // A phone's is a sheet over the page's left side; the rest of the screen is dimmed, and a
    // tap on it closes the sheet (the same view's command again toggles it shut).
    let mut scrim = Dom::create_div()
        .with_id(ids::STRIP_SCRIM)
        .with_css("flex-grow: 1; min-width: 0px; background: rgba(0,0,0,0.35);")
        .with_accessibility_info(AccessibilityInfo::named(
            label("azreader-close-strip"),
            AccessibilityRole::PushButton,
        ));
    scrim.add_callback(
        EventFilter::Hover(HoverEventFilter::Click),
        command(app, Command::Pane(st.pane)),
        on_command,
    );
    Some(
        Dom::create_div()
            .with_css(
                "position: absolute; top: 0px; bottom: 0px; left: 0px; right: 0px; display: flex; \
                 flex-direction: row;",
            )
            .with_child(sheet)
            .with_child(scrim),
    )
}

/// The thumbnail of page `page`: its paper at `scale` of its size, with the picture of the page
/// on it where the page is drawn.
fn thumbnail(
    st: &AppState,
    page: usize,
    geometry: &PageGeometry,
    colors: &PaperColors,
    size: (f32, f32),
    drawn: bool,
) -> Dom {
    let (tw, th) = size;
    let paper_w = geometry.text_width + 2.0 * geometry.margin;
    let scale = tw / paper_w.max(1.0);
    let mut paper = Dom::create_div().with_css(format!(
        "position: relative; overflow: hidden; width: {tw}px; height: {th}px; background: {}; \
         color: {};",
        colors.background, colors.text
    ));
    if let Some((run, k)) = st.pictures.find(page).filter(|_| drawn) {
        let (inner_w, inner_h) = (geometry.text_width * scale, geometry.text_height * scale);
        #[allow(clippy::cast_precision_loss)]
        let (top, total) = (k as f32 * inner_h, run.count as f32 * inner_h);
        paper.add_child(
            Dom::create_div()
                .with_css(format!(
                    "position: absolute; left: {}px; top: {}px; width: {inner_w}px; \
                     height: {inner_h}px; overflow: hidden;",
                    geometry.margin * scale,
                    geometry.margin * scale
                ))
                .with_child(Dom::create_image(run.image.clone()).with_css(format!(
                    "display: block; position: relative; top: -{top}px; width: {inner_w}px; \
                     height: {total}px;"
                ))),
        );
    }
    paper
}

/// The pages of the chapter on screen as a column of thumbnails; the pages in view are the
/// selected ones.
fn pages(app: &RefAny, st: &AppState) -> Dom {
    let Some(chapter) = st.chapter.as_ref().filter(|c| {
        st.open.as_ref().is_some_and(|o| o.info.id == c.book_id)
    }) else {
        return text(&t("azreader-laying-out-pages"), "padding: 12px 14px; font-size: 12px; color: system:secondary-text;");
    };
    let geometry = st.geometry();
    let colors = st.settings.paper.colors(st.dark, st.flora);
    let count = chapter.pages.page_count();
    // The thumbnail is the page's paper (its margins included) at the strip's width.
    let width = 92.0_f32;
    let paper_w = (geometry.text_width + 2.0 * geometry.margin).max(1.0);
    let paper_h = geometry.text_height + 2.0 * geometry.margin;
    let size = (width, (width * paper_h / paper_w).round().max(40.0));
    let drawn = st.pictures_key().is_some_and(|key| key == st.pictures.key);
    let per_view = geometry.per_view;
    let items: Vec<ThumbnailItem> = (0..count)
        .map(|page| {
            ThumbnailItem::create(
                thumbnail(st, page, &geometry, &colors, size, drawn),
                AzString::from((page + 1).to_string()),
                AzString::from(t_args("azreader-page-n", &[("n", Arg::from(page + 1))])),
            )
            .with_selected(page >= st.page && page < st.page + per_view)
        })
        .collect();
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; overflow-y: auto;")
        .with_child(
            ThumbnailStrip::create(ThumbnailItemVec::from_vec(items))
                .with_thumb_size(size.0, size.1)
                .with_active(st.page)
                .with_accessibility_name(label("azreader-thumbnails"))
                .with_on_event(app.clone(), on_thumbnail as ThumbnailStripOnEventCallbackType)
                .dom(),
        )
}

/// A thumbnail was chosen (a click, an arrow key, Enter): that page.
extern "C" fn on_thumbnail(mut data: RefAny, mut info: CallbackInfo, event: ThumbnailStripEvent) -> Update {
    match event.kind {
        ThumbnailStripEventKind::Select | ThumbnailStripEventKind::Activate => {
            let update = run(&mut data, Command::GoToPage(event.index), &mut info);
            close_on_a_phone(&mut data);
            update
        }
        // The pages are not the reader's to reorder, delete or fold.
        _ => Update::DoNothing,
    }
}

/// A phone's strip is a sheet: a choice in it closes it.
pub(crate) fn close_on_a_phone(data: &mut RefAny) {
    if let Some(mut st) = data.downcast_mut::<AppState>() {
        if st.phone {
            st.pane = Pane::None;
            st.area = None;
        }
    }
}

/// The table of contents.
fn contents(app: &RefAny, st: &AppState) -> Dom {
    let Some(open) = st.open.as_ref() else {
        return Dom::create_div();
    };
    let current = st.chapter.as_ref().map(|c| c.chapter);
    let current_entry = current.and_then(|c| open.book.toc.iter().position(|e| e.chapter == Some(c)));
    let mut pane = Dom::create_div()
        .with_id(ids::TOC)
        .with_css(PANE_CSS)
        .with_child(text(&t("azreader-pane-contents"), PANE_TITLE_CSS));
    for (i, entry) in open.book.toc.iter().enumerate() {
        let on = current_entry == Some(i);
        let mut row = Dom::create_div()
            .with_id(ids::indexed(&ids::TOC_ROW, i))
            .with_class(ids::TOC_ROW_CLASS)
            .with_css(format!(
                "padding: 8px 14px 8px {}px; font-size: 13px; min-height: 20px; {} {}",
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
                on_entry,
            );
        }
        pane.add_child(row);
    }
    pane
}

/// A row of the contents or a bookmark was chosen: its command runs, and a phone's strip closes.
extern "C" fn on_entry(data: RefAny, info: CallbackInfo) -> Update {
    let mut handle = data.clone();
    let update = on_command(data, info);
    close_on_a_phone(&mut handle);
    update
}

/// The bookmarks.
fn bookmarks(app: &RefAny, st: &AppState) -> Dom {
    let Some(open) = st.open.as_ref() else {
        return Dom::create_div();
    };
    let mut pane = Dom::create_div()
        .with_id(ids::BOOKMARKS)
        .with_css(PANE_CSS)
        .with_child(text(&t("azreader-pane-bookmarks"), PANE_TITLE_CSS));
    if open.state.bookmarks.is_empty() {
        pane.add_child(text(
            &t("azreader-no-bookmark"),
            "padding: 6px 14px; font-size: 12px; color: system:secondary-text;",
        ));
    }
    for (i, mark) in open.state.bookmarks.iter().enumerate() {
        let percent = position::percent_label(position::book_progress(&open.weights, mark.position));
        let mut body = Dom::create_div()
            .with_css("display: flex; flex-direction: column; flex-grow: 1; cursor: pointer; min-width: 0px;")
            .with_child(text(
                &format!("{} - {percent}", mark.chapter_title),
                "font-size: 11px; color: system:secondary-text;",
            ))
            .with_child(text(&mark.label, "font-size: 13px; color: system:text;"));
        body.add_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            command(app, Command::GoToBookmark(i)),
            on_entry,
        );
        let remove = Button::create("")
            .with_icon("close")
            .with_on_click(command(app, Command::RemoveBookmark(i)), on_command as ButtonOnClickCallbackType)
            .dom()
            .with_accessibility_assign(AccessibilityInfo::named(
                t_args("azreader-remove-named", &[("name", Arg::from(mark.label.as_str()))]),
                AccessibilityRole::PushButton,
            ));
        pane.add_child(
            Dom::create_div()
                .with_id(ids::indexed(&ids::BOOKMARK_ROW, i))
                .with_class(ids::BOOKMARK_ROW_CLASS)
                .with_css("display: flex; flex-direction: row; align-items: center; padding: 4px 8px 4px 14px;")
                .with_child(body)
                .with_child(remove),
        );
    }
    pane
}
