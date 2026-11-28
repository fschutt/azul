//! The document area and the status bar.
//!
//! Print layout: the A4 sheets on the canvas, each sheet ONE page of the
//! shared editor (`RichTextEditor::page_doms_at`; a paragraph a page cuts
//! shows its head on one sheet and its tail on the next) - every page edits the one
//! document and its one history. Web layout: one sheet with every block.
//! The paper follows the mode (the mode's background and text colours, as
//! the editor's own content colours do); the PDF is black on white.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CallbackInfo, RefAny, RichTextEditorOnChangeCallbackType,
        StatusBarOnViewSelectCallbackType, Update,
    },
    dom::{AttributeType, Dom},
    shells::ShellEmptyState,
    str::String as AzString,
    widgets::{
        RichTextEditor, RichTextEditorState, SliderState, StatusBar, StatusBarSegment,
        StatusBarView, StatusBarViewSwitcher, StatusBarZoom,
    },
};

use crate::{
    app::{command, AppState, Command, View, ZOOM_MAX, ZOOM_MIN},
    commands::on_command,
    ids, paginate,
};

/// A key of the resources said, or words as they are (a document's title).
fn s(text: &str) -> AzString {
    azul_appkit::l10n::label(text)
}

/// The empty state: no document is open.
fn empty_state(app: &RefAny) -> Dom {
    ShellEmptyState::create(s("azwriter-empty"))
        .with_icon(AzString::from("description"))
        .with_detail(s("azwriter-empty-what"))
        .with_action_label(s("azwriter-blank-document"))
        .with_on_action(
            command(app, Command::NewDocument),
            on_command as ButtonOnClickCallbackType,
        )
        .dom()
        .with_id(ids::EMPTY)
}

/// One sheet of paper around `page` (an editing host), `zoom` the scale. `under` (a blank sheet's placeholder) is the
/// sheet's first child, so the page that follows is painted over it and takes the touch.
fn sheet(page: Dom, width: f32, min_height: f32, margin: f32, under: Option<Dom>) -> Dom {
    let mut paper = Dom::create_div()
        .with_class(ids::SHEET)
        .with_css(format!(
            "display: block; position: relative; flex-shrink: 0; box-sizing: border-box; width: {width}px; \
             min-height: {min_height}px; padding: {margin}px; margin-bottom: 16px; \
             background: system:background; color: system:text; {} \
             border: 1px solid system:separator; box-shadow: 0px 1px 4px rgba(0, 0, 0, 0.25);",
            paginate::PAPER_TEXT_CSS
        ));
    if let Some(under) = under {
        paper.add_child(under);
    }
    paper.with_child(page)
}

/// The room inside a phone's sheet on each side, px: a phone's margin, not the print layout's inch.
const PHONE_PAGE_MARGIN: f32 = 20.0;

/// The least height of a phone's sheet, px: an empty page is a page, not one line of text.
const PHONE_SHEET_MIN_HEIGHT: f32 = 480.0;

/// Whether the document is blank: no word and at most the one empty block a new document is made of. Only then the
/// sheet says where to write.
#[must_use]
pub fn shows_placeholder(words: usize, blocks: usize) -> bool {
    words == 0 && blocks <= 1
}

/// What a blank sheet says: a caret where the text begins and "Start writing..." after it, under the editor (the
/// editor is transparent and takes the tap: the keyboard comes with the first touch, not before). The caret is the
/// editor's own once the editor has the focus.
fn placeholder(margin: f32) -> Dom {
    Dom::create_div()
        .with_id(ids::PLACEHOLDER)
        .with_css(format!(
            "position: absolute; left: {margin}px; top: {margin}px; display: flex; flex-direction: row; \
             align-items: center; color: system:tertiary-text; {}",
            paginate::PAPER_TEXT_CSS
        ))
        .with_child(
            Dom::create_div()
                .with_css("width: 2px; height: 20px; margin-right: 3px; background: system:accent;"),
        )
        .with_child(Dom::create_span_with_text(s("azwriter-placeholder")))
}

/// Page `index`'s editing host; on a desktop the first one takes the keyboard when it mounts, so a
/// document opens with the caret in its text. On a phone it does not (`takes` false): the soft keyboard
/// covers half the screen and comes with the first touch on the page, not at launch.
fn first_takes_the_keyboard(index: usize, page: &Dom, takes: bool) -> Dom {
    if index == 0 && takes {
        page.clone().with_attribute(AttributeType::Autofocus)
    } else {
        page.clone()
    }
}

/// The canvas's room beside a sheet, both sides together, px.
const CANVAS_SIDES: f32 = 48.0;

/// The same on a phone: the sheet fills the screen and a hair of canvas on each side shows its edge.
const PHONE_CANVAS_SIDES: f32 = 16.0;

/// Whether a window `window_width` wide is a phone's (`ribbon::PHONE_WIDTH`).
#[must_use]
fn is_phone_width(window_width: f32) -> bool {
    window_width < crate::ribbon::PHONE_WIDTH
}

/// The narrowest sheet of the web layout: a phone is 320 dp wide at the least, and the sheet leaves the canvas its
/// [`CANVAS_SIDES`] (the sheet was never narrower than 320, and ran out of a 320 dp window).
const MIN_WEB_SHEET: f32 = 240.0;

/// The width of the web layout's sheet in a window `window_width` wide at `zoom`: the window less the canvas's
/// room beside it, at most 1200 px at 100 %.
#[must_use]
pub fn web_sheet_width(window_width: f32, zoom: f32) -> f32 {
    let sides = if is_phone_width(window_width) { PHONE_CANVAS_SIDES } else { CANVAS_SIDES };
    (window_width - sides).clamp(MIN_WEB_SHEET, 1200.0 * zoom)
}

/// Whether the document is shown as the web layout's one sheet: the web layout was asked for, or
/// the window is narrower than the A4 sheet the print layout would put in it (a phone: a sheet of
/// 794 px ran out of a 411 dp screen and its text with it). The sheet that fits the screen reflows.
#[must_use]
pub fn shows_as_web(view: View, window_width: f32, zoom: f32) -> bool {
    view == View::Web || window_width < paginate::A4_W * zoom + CANVAS_SIDES
}

/// The document area for the app's state (`window_width`: the room the
/// web layout's sheet may take).
#[must_use]
pub fn document_area(app: &RefAny, st: &AppState, window_width: f32) -> Dom {
    let Some(doc) = st.doc.as_ref() else {
        return empty_state(app);
    };
    let zoom = st.zoom_percent / 100.0;
    let editor = RichTextEditor::create(doc.editor.clone())
        .with_id(ids::DOC_HOST)
        .with_accessibility_name(s("azwriter-document"))
        .with_font_size((paginate::FONT_PX * zoom).round())
        // The body is 11 pt (FONT_PX at 100 %): a run's own size in points scales with it.
        .with_body_points(crate::ribbon::DEFAULT_FONT_SIZE)
        .with_paragraph_spacing((paginate::SPACING_PX * zoom).round())
        .with_on_change(app.clone(), on_editor_change as RichTextEditorOnChangeCallbackType);
    let mut canvas = Dom::create_div().with_id(ids::CANVAS).with_css(
        "display: flex; flex-direction: column; align-items: center; flex-grow: 1; \
         min-height: 0px; overflow-y: auto; padding-top: 18px; \
         background: system:under-page-background;",
    );
    let margin = (paginate::MARGIN * zoom).round();
    let view = if shows_as_web(st.view, window_width, zoom) { View::Web } else { View::Print };
    match view {
        View::Print => {
            let starts = st.pages.starts_for(doc.doc().block_count());
            let count = starts.len();
            let pages = editor.page_doms_at(paginate::Pages::page_starts(&starts), 0, count);
            for (i, page) in pages.as_slice().iter().enumerate() {
                canvas.add_child(sheet(
                    first_takes_the_keyboard(i, page, true),
                    (paginate::A4_W * zoom).round(),
                    (paginate::A4_H * zoom).round(),
                    margin,
                    None,
                ));
            }
        }
        View::Web => {
            let pages = editor.page_doms(vec![0u32], 0, 1);
            let width = web_sheet_width(window_width, zoom);
            let phone = is_phone_width(window_width);
            let inside = if phone { PHONE_PAGE_MARGIN } else { (margin / 2.0).round() };
            let blank = shows_placeholder(doc.word_count(), doc.doc().block_count());
            for (i, page) in pages.as_slice().iter().enumerate() {
                // A phone's sheet fills the screen (a page of its own, with its margins and edge); its keyboard
                // comes with the first touch.
                let page = first_takes_the_keyboard(i, page, !crate::ribbon::draws_under_the_bars());
                let under = (i == 0 && blank).then(|| placeholder(inside));
                canvas.add_child(sheet(
                    page,
                    width.round(),
                    if phone { PHONE_SHEET_MIN_HEIGHT } else { 0.0 },
                    inside,
                    under,
                ));
            }
        }
    }
    canvas
}

/// The shared editor's new state (typing, a structural edit, a key
/// command, an undo): the open document's. The status bar's word count
/// follows at once; the window is rebuilt only when the title's "unsaved"
/// star appears.
pub extern "C" fn on_editor_change(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: RichTextEditorState,
) -> Update {
    let handle = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    let Some(doc) = st.doc.as_mut() else {
        return Update::DoNothing;
    };
    let was_dirty = doc.is_dirty();
    doc.adopt(state);
    let dirty = doc.is_dirty();
    let label = azul_appkit::l10n::t_args(
        "azwriter-status-words",
        &[("count", azul_appkit::l10n::Arg::from(doc.word_count()))],
    );
    if let Some(node) = info.get_node_id_by_marker(st.word_count_marker.clone()).into_option() {
        let _ = StatusBar::update_segment_label(info, node, label);
    }
    paginate::ensure(st, &mut info, &handle);
    if dirty != was_dirty {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

extern "C" fn on_view_select(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let view = View::ALL.get(index).copied().unwrap_or(View::Print);
    crate::commands::run(&mut data, Command::View(view), &mut info)
}

extern "C" fn on_zoom_slider(mut data: RefAny, _info: CallbackInfo, slider: SliderState) -> Update {
    let Some(mut st) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    st.zoom_percent = slider.value.round().clamp(ZOOM_MIN, ZOOM_MAX);
    Update::RefreshDom
}

/// The status bar: the caret's page, the words, the notice; the views and
/// the zoom.
#[must_use]
pub fn status_bar(app: &RefAny, st: &AppState, phone: bool) -> Dom {
    let (page_label, words) = match st.doc.as_ref() {
        Some(doc) => {
            let pages = st.pages.starts_for(doc.doc().block_count()).len();
            let page = st
                .pages
                .page_of(doc.editor.caret_block, doc.editor.caret_byte)
                .min(pages.saturating_sub(1))
                + 1;
            (
                azul_appkit::l10n::t_args(
                    "azwriter-status-page",
                    &[
                        ("page", azul_appkit::l10n::Arg::from(page)),
                        ("pages", azul_appkit::l10n::Arg::from(pages)),
                    ],
                ),
                doc.word_count(),
            )
        }
        None => (azul_appkit::l10n::t("azwriter-status-no-document"), 0),
    };
    let mut segments = vec![
        StatusBarSegment::create(s(&page_label)),
        StatusBarSegment::create(AzString::from(azul_appkit::l10n::t_args(
            "azwriter-status-words",
            &[("count", azul_appkit::l10n::Arg::from(words))],
        )))
        .with_marker(st.word_count_marker.clone()),
    ];
    if st.is_saving() {
        segments.push(StatusBarSegment::create(s("azwriter-status-saving")).with_icon(AzString::from("cloud_upload")));
    }
    if !st.notice.is_empty() {
        segments.push(StatusBarSegment::create(s(&st.notice)));
    }
    // A phone shows the page and the words and nothing to press: the print layout does not fit it (the web
    // layout is the only one), the zoom is the system's font size, and the view keys and the zoom's keys and
    // slider are 20 px controls, none of them a finger's.
    if phone {
        return StatusBar::create(segments).dom();
    }
    let views: Vec<StatusBarView> = View::ALL
        .iter()
        .map(|v| StatusBarView { icon: s(v.icon()) })
        .collect();
    let zoom = StatusBarZoom::create(st.zoom_percent, ZOOM_MIN, ZOOM_MAX)
        .with_on_zoom_out(command(app, Command::Zoom(-10)), on_command as ButtonOnClickCallbackType)
        .with_on_zoom_in(command(app, Command::Zoom(10)), on_command as ButtonOnClickCallbackType)
        .with_on_slider_change(app.clone(), on_zoom_slider);
    StatusBar::create(segments)
        .with_views(
            StatusBarViewSwitcher::create(views)
                .with_active_view(st.view.index())
                .with_on_select(app.clone(), on_view_select as StatusBarOnViewSelectCallbackType),
        )
        .with_zoom(zoom)
        .dom()
}

#[cfg(test)]
#[path = "pages_test.rs"]
mod pages_test;
