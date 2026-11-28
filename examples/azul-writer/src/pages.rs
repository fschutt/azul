//! The document area and the status bar.
//!
//! The text FLOWS by default (the flow editor of azul-appkit: one column of a comfortable measure
//! on the window's own ground, like a mobile Markdown editor): one editing host with every block.
//! The page layout is a View option: the A4 sheets on the canvas, each sheet ONE page of the
//! shared editor (`RichTextEditor::page_doms_at`; a paragraph a page cuts shows its head on one
//! sheet and its tail on the next) - every page edits the one document and its one history. A
//! window narrower than a sheet shows the flow whatever the view says. The paper follows the mode
//! (the mode's background and text colours, as the editor's own content colours do); the PDF is
//! black on white.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CallbackInfo, RefAny, RichTextEditorOnChangeCallbackType,
        StatusBarOnViewSelectCallbackType, Update,
    },
    dom::Dom,
    shells::ShellEmptyState,
    str::String as AzString,
    widgets::{
        RichTextEditor, RichTextEditorState, SliderState, StatusBar, StatusBarSegment,
        StatusBarView, StatusBarViewSwitcher, StatusBarZoom,
    },
};
use azul_appkit::flow::{self, FlowOptions, FlowPaper};

use crate::{
    app::{command, AppState, Command, View, VIEWS, ZOOM_MAX, ZOOM_MIN},
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

/// The least height of a phone's page, px: an empty page is a page, not one line of text, and a
/// tap anywhere on it is a tap into the text.
const PHONE_PAGE_MIN_HEIGHT: f32 = 480.0;

/// Whether the document is blank: no word and at most the one empty block a new document is made of. Only then the
/// page says where to write.
#[must_use]
pub fn shows_placeholder(words: usize, blocks: usize) -> bool {
    words == 0 && blocks <= 1
}

/// The canvas's room beside a sheet, both sides together, px.
const CANVAS_SIDES: f32 = 48.0;

/// Whether the document is shown as A4 sheets: the page layout was asked for AND the window holds a
/// sheet (a phone's 411 dp are narrower than the 794 px of an A4 sheet: it ran out of the screen and its text
/// with it). Everything else is the flow.
#[must_use]
pub fn shows_pages(view: View, window_width: f32, zoom: f32) -> bool {
    view == View::Pages && window_width >= paginate::A4_W * zoom + CANVAS_SIDES
}

/// What the document area needs to know about its window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    /// The width the area has, px.
    pub width: f32,
    /// Whether this is a phone's layout (narrower margins, the keyboard comes with the first tap).
    pub phone: bool,
    /// The room the soft keyboard takes at the bottom, px (the last line scrolls above it).
    pub lift: f32,
}

/// The document area for the app's state.
#[must_use]
pub fn document_area(app: &RefAny, st: &AppState, area: Area) -> Dom {
    let Some(doc) = st.doc() else {
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
    // a desktop's text takes the keyboard when it opens; a phone's comes with the first tap
    let autofocus = !area.phone;
    if shows_pages(st.view, area.width, zoom) {
        let starts = doc.pages.starts_for(doc.doc().block_count());
        let count = starts.len();
        let pages = editor.page_doms_at(paginate::Pages::page_starts(&starts), 0, count);
        let o = FlowOptions {
            dom_id: ids::CANVAS.as_str().to_string(),
            paper_class: ids::SHEET.as_str().to_string(),
            paper: FlowPaper::Sheet,
            font_css: paginate::PAPER_TEXT_CSS.to_string(),
            width: (paginate::A4_W * zoom).round(),
            min_height: (paginate::A4_H * zoom).round(),
            margin: (paginate::MARGIN * zoom).round(),
            lift: area.lift,
            phone: false,
            blank: false,
            placeholder: String::new(),
            autofocus,
        };
        return flow::canvas_pages(pages.as_slice().to_vec(), &o);
    }
    let pages = editor.page_doms(vec![0u32], 0, 1);
    let page = pages.as_slice().first().cloned().unwrap_or_else(Dom::create_div);
    let o = FlowOptions {
        dom_id: ids::CANVAS.as_str().to_string(),
        paper_class: ids::SHEET.as_str().to_string(),
        paper: FlowPaper::Plain,
        font_css: paginate::PAPER_TEXT_CSS.to_string(),
        width: flow::DEFAULT_MEASURE,
        min_height: if area.phone { PHONE_PAGE_MIN_HEIGHT } else { 0.0 },
        margin: 0.0,
        lift: area.lift,
        phone: area.phone,
        blank: shows_placeholder(doc.word_count(), doc.doc().block_count()),
        placeholder: azul_appkit::l10n::t("azwriter-placeholder"),
        autofocus,
    };
    flow::canvas(page, &o)
}

/// The shared editor's new state (typing, a structural edit, a key
/// command, an undo): the shown document's. The status bar's word count
/// follows at once; the window is rebuilt only when the document's sync pie changes
/// (its "unsaved" state appears).
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
    let index = st.active;
    let Some(doc) = st.open.get_mut(index) else {
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
    let view = VIEWS.get(index).copied().unwrap_or(View::Flow);
    crate::commands::run(&mut data, Command::View(view), &mut info)
}

extern "C" fn on_zoom_slider(mut data: RefAny, _info: CallbackInfo, slider: SliderState) -> Update {
    let Some(mut st) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    st.zoom_percent = slider.value.round().clamp(ZOOM_MIN, ZOOM_MAX);
    Update::RefreshDom
}

/// The status bar, the frame's footer: the caret's page, the words, the notice; the views and
/// the zoom on a desktop.
#[must_use]
pub fn status_bar(app: &RefAny, st: &AppState, phone: bool) -> Dom {
    let (page_label, words) = match st.doc() {
        Some(doc) => {
            let pages = doc.pages.starts_for(doc.doc().block_count()).len();
            let page = doc
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
    // A phone shows the page and the words and nothing to press: the zoom is the system's text size, and the view keys and
    // the zoom's keys and slider are 20 px controls, none of them a finger's.
    if phone {
        return StatusBar::create(segments).dom();
    }
    let views: Vec<StatusBarView> = VIEWS
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
