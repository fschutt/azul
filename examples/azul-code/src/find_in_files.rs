//! The side bar's SEARCH (Mod+Shift+F): VSCode's search over the folder -
//! the query (every key searches again; the search that runs is stopped),
//! match case, whole word, "12 results in 3 files", and the results: each
//! file (a click folds its matches), its matching lines with the match
//! marked (a click opens the file with the match selected). The list is
//! VIRTUALIZED like the explorer (a `VirtualView` that builds the rows in
//! view): a search can find 20,000 matches.
//!
//! The search runs on a Thread through the workspace's drive
//! ([`crate::storage::search_files`]); the find bar (Mod+F) searches the
//! file in front.

use azul::{
    callbacks::{ButtonOnClickCallbackType, TextInputOnTextInputCallbackType, TextInputOnVirtualKeyDownCallbackType},
    dom::VirtualKeyCode,
    prelude::*,
    str::String as AzString,
    widgets::{Button, OnTextInputReturn, TextInput, TextInputState, TextInputValid},
};

use crate::{
    app::{AppState, ResultRow},
    commands,
    explorer::{window_of, ROW_HEIGHT},
    ids,
    ui::{self, column, label, side_title},
    workspace::{file_name, parent_folder},
};

/// What the results' view hands its callback: the app.
struct ResultsRef {
    app: RefAny,
}

/// What a result row acts on.
struct ResultRef {
    app: RefAny,
    row: ResultRow,
}

/// The search panel: the field, the toggles, the summary, the results.
pub fn search_panel(app: &RefAny, st: &AppState) -> Dom {
    let field = TextInput::create()
        .with_text(AzString::from(st.search.query.as_str()))
        .with_placeholder(AzString::from("Search"))
        .with_accessibility_name(AzString::from("Search the folder"))
        .with_on_text_input(app.clone(), on_search_text as TextInputOnTextInputCallbackType)
        .with_on_virtual_key_down(app.clone(), on_search_key as TextInputOnVirtualKeyDownCallbackType)
        .dom()
        .with_id(ids::SEARCH_INPUT);
    let toggles = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; padding-top: 4px;")
        .with_child(
            Button::create(AzString::from("Aa"))
                .with_toggled(st.search.how.match_case)
                .with_on_click(app.clone(), on_match_case as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::SEARCH_MATCH_CASE)
                .with_accessibility_name("Match Case"),
        )
        .with_child(
            Button::create(AzString::from("ab"))
                .with_toggled(st.search.how.whole_word)
                .with_on_click(app.clone(), on_whole_word as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::SEARCH_WHOLE_WORD)
                .with_accessibility_name("Match Whole Word"),
        );
    let summary = match st.workspace {
        None => "Open a folder to search its files.".to_string(),
        Some(_) => st.search.summary(),
    };
    let results = Dom::create_virtual_view(RefAny::new(ResultsRef { app: app.clone() }), render_results)
        .with_id(ids::SEARCH_RESULTS)
        .with_css("flex-grow: 1; min-height: 0px; width: 100%;")
        .with_accessibility_name("Search results");
    column(vec![
        side_title("SEARCH", Vec::new()),
        Dom::create_div()
            .with_css("display: flex; flex-direction: column; flex-shrink: 0; padding: 0px 12px 6px 20px;")
            .with_child(field)
            .with_child(toggles),
        label(&summary, "flex-shrink: 0; font-size: 12px; opacity: 0.75; padding: 2px 12px 6px 20px;")
            .with_id(ids::SEARCH_SUMMARY),
        results,
    ])
    .with_id(ids::SEARCH_PANEL)
}

/// A rect of the view, px.
fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect::create(LogicalPosition::create(x, y), LogicalSize::create(w, h))
}

/// The results' `VirtualView`: the rows in view and a screen either side.
extern "C" fn render_results(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let Some(mut app) = data.downcast_ref::<ResultsRef>().map(|r| r.app.clone()) else {
        return VirtualViewReturn::default();
    };
    let handle = app.clone();
    let Some(guard) = app.downcast_ref::<AppState>() else {
        return VirtualViewReturn::default();
    };
    let st = &*guard;
    let rows = st.search.rows();
    let size = info.bounds.get_logical_size();
    let width = size.width.max(1.0);
    let height = size.height.max(1.0);
    #[allow(clippy::cast_precision_loss)]
    let total = (rows.len() as f32 * ROW_HEIGHT).max(1.0);
    let (first, end) = window_of(rows.len(), info.scroll_offset.y, height);
    let mut root = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: column; width: {width}px;"
    ));
    for (i, row) in rows[first..end].iter().enumerate() {
        root.add_child(row_dom(&handle, st, first + i, *row));
    }
    #[allow(clippy::cast_precision_loss)]
    let top = first as f32 * ROW_HEIGHT;
    #[allow(clippy::cast_precision_loss)]
    let built = ((end - first) as f32 * ROW_HEIGHT).max(1.0);
    VirtualViewReturn::with_dom(root, rect(0.0, top, width, built), rect(0.0, 0.0, width, total))
}

/// Result row `index`: a file (its name, its folder, its count) or a match
/// (the line, the match marked).
fn row_dom(app: &RefAny, st: &AppState, index: usize, row: ResultRow) -> Dom {
    let base = format!(
        "display: flex; flex-direction: row; align-items: center; height: {ROW_HEIGHT}px; flex-shrink: 0; \
         padding-right: 8px; cursor: pointer; white-space: nowrap; overflow: hidden; font-size: 13px; \
         :hover {{ background: rgba(128, 128, 128, 0.12); }}"
    );
    let dom = Dom::create_div()
        .with_id(ids::result_row(index))
        .with_class(ids::RESULT_CLASS)
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            RefAny::new(ResultRef { app: app.clone(), row }),
            on_result_click,
        );
    match row {
        ResultRow::File(f) => {
            let Some(file) = st.search.results.get(f) else {
                return dom;
            };
            let folded = st.search.folded.contains(&file.key);
            let folder = parent_folder(&file.key).unwrap_or("").trim_end_matches('/').to_string();
            dom.with_css(format!("{base} padding-left: 8px;"))
                .with_accessibility_name(format!("{}, {} matches", file.key, file.hits.len()))
                .with_child(
                    Dom::create_icon(if folded { "chevron_right" } else { "expand_more" })
                        .with_css("font-size: 16px; width: 16px; flex-shrink: 0;"),
                )
                .with_child(Dom::create_icon("description").with_css("font-size: 16px; padding: 0px 6px 0px 2px; opacity: 0.8;"))
                .with_child(label(file_name(&file.key), "flex-shrink: 0; padding-right: 6px;"))
                .with_child(label(&folder, "font-size: 12px; opacity: 0.6; min-width: 0px; overflow: hidden; flex-grow: 1;"))
                .with_child(label(
                    &file.hits.len().to_string(),
                    "font-size: 11px; padding: 0px 6px; border-radius: 8px; background: rgba(128, 128, 128, 0.25);",
                ))
        }
        ResultRow::Hit(f, h) => {
            let Some(hit) = st.search.results.get(f).and_then(|file| file.hits.get(h)) else {
                return dom;
            };
            let before = &hit.preview[..hit.preview_start];
            let matched = &hit.preview[hit.preview_start..hit.preview_end];
            let after = &hit.preview[hit.preview_end..];
            dom.with_css(format!("{base} padding-left: 40px;"))
                .with_accessibility_name(format!("Line {}: {}", hit.line + 1, hit.preview))
                .with_child(Dom::create_span_with_text(before))
                .with_child(
                    Dom::create_span_with_text(matched)
                        .with_css("background: rgba(234, 92, 0, 0.33); border-radius: 2px;"),
                )
                .with_child(Dom::create_span_with_text(after))
        }
    }
}

/// A result's click: a file's row folds or unfolds its matches; a match
/// opens its file with the match selected.
extern "C" fn on_result_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, row)) = data.downcast_ref::<ResultRef>().map(|r| (r.app.clone(), r.row)) else {
        return Update::DoNothing;
    };
    ui::with_state(&mut app, &mut info, |st, info, app| match row {
        ResultRow::File(f) => {
            if let Some(key) = st.search.results.get(f).map(|file| file.key.clone()) {
                if !st.search.folded.remove(&key) {
                    st.search.folded.insert(key);
                }
            }
        }
        ResultRow::Hit(f, h) => {
            let target = st
                .search
                .results
                .get(f)
                .and_then(|file| file.hits.get(h).map(|hit| (file.key.clone(), hit.found())));
            if let Some((key, found)) = target {
                commands::open_file_at(st, info, app, &key, found);
            }
        }
    })
}

/// The field: every key searches again.
extern "C" fn on_search_text(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let query = state.get_text().as_str().to_string();
    let update = ui::with_state(&mut data, &mut info, |st, info, app| {
        if st.search.query != query {
            st.search.query = query;
            commands::search_folder(st, info, app);
        }
    });
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// Enter searches again (the folder may have changed); Escape gives the
/// editor the keys.
extern "C" fn on_search_key(mut data: RefAny, mut info: CallbackInfo, _state: TextInputState) -> OnTextInputReturn {
    let key = info.get_current_keyboard_state().current_virtual_keycode.into_option();
    let update = match key {
        Some(VirtualKeyCode::Return) => {
            info.prevent_default();
            ui::with_state(&mut data, &mut info, commands::search_folder)
        }
        Some(VirtualKeyCode::Escape) => {
            info.prevent_default();
            ui::with_state(&mut data, &mut info, |st, info, _| {
                if st.tabs.active().is_some() {
                    commands::focus_soon(info, ids::EDITOR.as_str());
                }
            })
        }
        _ => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_match_case(mut data: RefAny, mut info: CallbackInfo) -> Update {
    ui::with_state(&mut data, &mut info, |st, info, app| {
        st.search.how.match_case = !st.search.how.match_case;
        commands::search_folder(st, info, app);
    })
}

extern "C" fn on_whole_word(mut data: RefAny, mut info: CallbackInfo) -> Update {
    ui::with_state(&mut data, &mut info, |st, info, app| {
        st.search.how.whole_word = !st.search.how.whole_word;
        commands::search_folder(st, info, app);
    })
}
