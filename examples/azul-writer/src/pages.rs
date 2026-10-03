//! The document area and the status bar.
//!
//! Print layout: the A4 sheets on the canvas, each sheet ONE page of the
//! shared editor (`RichTextEditor::page_doms`) - every page edits the one
//! document and its one history. Web layout: one sheet with every block.
//! The paper follows the mode (the mode's background and text colours, as
//! the editor's own content colours do); the PDF is black on white.

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

use crate::{
    app::{command, AppState, Command, View, ZOOM_MAX, ZOOM_MIN},
    commands::on_command,
    ids, paginate,
};

fn s(text: &str) -> AzString {
    AzString::from(text)
}

/// The empty state: no document is open.
fn empty_state(app: &RefAny) -> Dom {
    ShellEmptyState::create(s("No document is open"))
        .with_icon(s("description"))
        .with_detail(s("Start a blank document, or open one from File."))
        .with_action_label(s("Blank document"))
        .with_on_action(
            command(app, Command::NewDocument),
            on_command as ButtonOnClickCallbackType,
        )
        .dom()
        .with_id(ids::EMPTY)
}

/// One sheet of paper around `page` (an editing host), `zoom` the scale.
fn sheet(page: Dom, width: f32, min_height: f32, margin: f32) -> Dom {
    Dom::create_div()
        .with_class(ids::SHEET)
        .with_css(format!(
            "display: block; flex-shrink: 0; box-sizing: border-box; width: {width}px; \
             min-height: {min_height}px; padding: {margin}px; margin-bottom: 16px; \
             background: system:background; color: system:text; {} \
             border: 1px solid system:separator;",
            paginate::PAPER_TEXT_CSS
        ))
        .with_child(page)
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
        .with_accessibility_name("Document")
        .with_font_size((paginate::FONT_PX * zoom).round())
        .with_paragraph_spacing((paginate::SPACING_PX * zoom).round())
        .with_on_change(app.clone(), on_editor_change as RichTextEditorOnChangeCallbackType);
    let mut canvas = Dom::create_div().with_id(ids::CANVAS).with_css(
        "display: flex; flex-direction: column; align-items: center; flex-grow: 1; \
         min-height: 0px; overflow-y: auto; padding-top: 18px; \
         background: system:under-page-background;",
    );
    let margin = (paginate::MARGIN * zoom).round();
    match st.view {
        View::Print => {
            let starts = st.pages.starts_for(doc.doc().block_count());
            let count = starts.len();
            let pages = editor.page_doms(starts, 0, count);
            for page in pages.as_slice() {
                canvas.add_child(sheet(
                    page.clone(),
                    (paginate::A4_W * zoom).round(),
                    (paginate::A4_H * zoom).round(),
                    margin,
                ));
            }
        }
        View::Web => {
            let pages = editor.page_doms(vec![0u32], 0, 1);
            let width = (window_width - 48.0).clamp(320.0, 1200.0 * zoom);
            for page in pages.as_slice() {
                canvas.add_child(sheet(page.clone(), width.round(), 0.0, (margin / 2.0).round()));
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
    let label = format!("{} WORDS", doc.word_count());
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
pub fn status_bar(app: &RefAny, st: &AppState) -> Dom {
    let (page_label, words) = match st.doc.as_ref() {
        Some(doc) => {
            let pages = st.pages.starts_for(doc.doc().block_count()).len();
            let page = st.pages.page_of(doc.editor.caret_block).min(pages.saturating_sub(1)) + 1;
            (format!("PAGE {page} OF {pages}"), doc.word_count())
        }
        None => ("NO DOCUMENT".to_string(), 0),
    };
    let mut segments = vec![
        StatusBarSegment::create(s(&page_label)),
        StatusBarSegment::create(s(&format!("{words} WORDS"))).with_marker(st.word_count_marker.clone()),
    ];
    if st.is_saving() {
        segments.push(StatusBarSegment::create(s("SAVING")).with_icon(s("cloud_upload")));
    }
    if !st.notice.is_empty() {
        segments.push(StatusBarSegment::create(s(&st.notice)));
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
