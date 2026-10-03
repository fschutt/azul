//! The window: the S8 DeveloperShell - the activity bar, the side bar (the
//! explorer or the search results), the editor (the tabs, the find and
//! go-to bars, azul's CodeView over the file in front) and the status bar -
//! and its callbacks.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CodeViewDataSourceCallbackType, CodeViewOnEventCallbackType,
        TabOnClickCallbackType, TextInputOnTextInputCallbackType, TextInputOnVirtualKeyDownCallbackType,
        TreeViewOnNodeClickCallbackType, TreeViewOnNodeToggleCallbackType,
    },
    dom::VirtualKeyCode,
    prelude::*,
    shells::{DeveloperShell, ShellEmptyState},
    str::String as AzString,
    widgets::{
        Button, CodeView, CodeViewEvent, CodeViewEventKind, OnTextInputReturn, StatusBar, StatusBarSegment,
        TabHeader, TabHeaderState, TextInput, TextInputState, TextInputValid, TreeView, TreeViewNode,
    },
};
use azul_appkit::ui as kit;

use crate::{
    app::{doc_line, AppState, Doc, Side},
    commands, ids,
    workspace::tab_label,
};

/// The columns between two tab stops.
pub const TAB_WIDTH: u32 = 4;
/// The most search results the side bar lists.
const MAX_RESULTS: usize = 200;

/// Runs `f` on the app's state; the window is rebuilt afterwards.
pub fn with_state(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    f: impl FnOnce(&mut AppState, &mut CallbackInfo, &RefAny),
) -> Update {
    let handle = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    f(&mut guard, info, &handle);
    Update::RefreshDom
}

fn text(content: &str) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 0px 6px;")
        .with_child(Dom::create_span_with_text(content))
}

// ==== The window ====

/// The whole window's content (under the theme scope).
pub fn window(app: &RefAny, st: &AppState) -> Dom {
    let title = st.title();
    if st.workspace.is_none() {
        return column(vec![kit::title_row(&title), welcome(app, st)]);
    }
    let side = match st.side {
        Side::Explorer => explorer(app, st),
        Side::Search => search_panel(app, st),
    };
    DeveloperShell::create(activity_bar(app, st), side, editor(app, st))
        .with_side_bar_ratio(0.22)
        .office_shell()
        .with_title_row(kit::title_row(&title))
        .with_status_bar(status_bar(st))
        .dom()
}

/// A column that fills its parent.
pub fn column(children: Vec<Dom>) -> Dom {
    let mut c = Dom::create_div().with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;");
    for child in children {
        c.add_child(child);
    }
    c
}

/// No workspace yet: what AzCode opens.
fn welcome(app: &RefAny, st: &AppState) -> Dom {
    let detail = if st.notice.is_empty() {
        "Start AzCode with a folder (AzCode ~/my-project) to edit its files, or open the sample: a small \
         Rust crate and a file of 100,000 lines."
            .to_string()
    } else {
        st.notice.clone()
    };
    ShellEmptyState::create(AzString::from("AzCode"))
        .with_icon(AzString::from("code"))
        .with_detail(AzString::from(detail))
        .with_action_label(AzString::from("Open the sample workspace"))
        .with_on_action(app.clone(), on_open_sample as ButtonOnClickCallbackType)
        .dom()
        .with_id(ids::WELCOME)
}

/// The activity bar: the explorer, the search.
fn activity_bar(app: &RefAny, st: &AppState) -> Dom {
    column(vec![
        Button::create(AzString::from("Files"))
            .with_toggled(st.side == Side::Explorer)
            .with_on_click(app.clone(), on_show_explorer as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::ACTIVITY_EXPLORER),
        Button::create(AzString::from("Find"))
            .with_toggled(st.side == Side::Search)
            .with_on_click(app.clone(), on_show_search as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::ACTIVITY_SEARCH),
    ])
}

/// The explorer: the workspace's tree, folders opened as they are listed.
fn explorer(app: &RefAny, st: &AppState) -> Dom {
    let Some(w) = st.workspace.as_ref() else {
        return Dom::create_div();
    };
    let rows = w.rows();
    // The rows are depth first; a stack of (depth, node) builds the tree.
    let mut stack: Vec<(usize, TreeViewNode)> = vec![(
        0,
        TreeViewNode::create(AzString::from(w.root.name.as_str()))
            .with_icon(AzString::from("folder_open"))
            .with_expanded(true),
    )];
    for row in &rows {
        let level = row.depth + 1;
        while stack.len() > level {
            let (_, done) = stack.pop().expect("deeper than the root");
            if let Some((_, parent)) = stack.last_mut() {
                parent.add_child(done);
            }
        }
        let selected = w.selected.as_deref() == Some(row.key.as_str());
        let mut node = TreeViewNode::create(AzString::from(row.name.as_str()))
            .with_icon(AzString::from(if row.folder { "folder" } else { "description" }))
            .with_selected(selected);
        if row.folder {
            node = node.with_expanded(row.expanded).with_unloaded_children(!row.expanded);
        }
        stack.push((level, node));
    }
    while stack.len() > 1 {
        let (_, done) = stack.pop().expect("deeper than the root");
        if let Some((_, parent)) = stack.last_mut() {
            parent.add_child(done);
        }
    }
    let (_, root) = stack.pop().expect("the root");
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; overflow-y: auto;")
        .with_child(
            TreeView::create(root)
                .with_on_node_click(app.clone(), on_tree_click as TreeViewOnNodeClickCallbackType)
                .with_on_node_toggle(app.clone(), on_tree_toggle as TreeViewOnNodeToggleCallbackType)
                .dom()
                .with_id(ids::EXPLORER),
        )
}

/// The search panel: the query, the toggles, the matches in the file in
/// front (each a button that selects it).
fn search_panel(app: &RefAny, st: &AppState) -> Dom {
    let mut children = vec![find_input(app, st), toggles(app, st)];
    let count = st.find.found.len();
    children.push(text(&match count {
        0 if st.find.query.is_empty() => "Type to search the file in front".to_string(),
        0 => "No results".to_string(),
        1 => "1 result".to_string(),
        n => format!("{n} results"),
    }));
    if let Some(doc) = st.tabs.active() {
        let lines: Vec<(usize, String)> = st
            .find
            .found
            .iter()
            .take(MAX_RESULTS)
            .map(|f| {
                let line = doc
                    .with_text(|t| t.buffer.line(f.line))
                    .unwrap_or_default();
                let shown: String = line.trim().chars().take(60).collect();
                (f.line, shown)
            })
            .collect();
        for (i, (line, shown)) in lines.into_iter().enumerate() {
            children.push(
                Button::create(AzString::from(format!("{}: {shown}", line + 1)))
                    .with_on_click(
                        RefAny::new(MatchRef { app: app.clone(), index: i }),
                        on_result_click as ButtonOnClickCallbackType,
                    )
                    .dom(),
            );
        }
    }
    column(children).with_id(ids::SEARCH_PANEL)
}

/// The find field (shared by the find bar and the search panel).
fn find_input(app: &RefAny, st: &AppState) -> Dom {
    TextInput::create()
        .with_text(AzString::from(st.find.query.as_str()))
        .with_placeholder(AzString::from("Find"))
        .with_accessibility_name(AzString::from("Find"))
        .with_on_text_input(app.clone(), on_find_text as TextInputOnTextInputCallbackType)
        .with_on_virtual_key_down(app.clone(), on_find_key as TextInputOnVirtualKeyDownCallbackType)
        .dom()
        .with_id(ids::FIND_INPUT)
}

/// Match case, whole word.
fn toggles(app: &RefAny, st: &AppState) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center;")
        .with_child(
            Button::create(AzString::from("Aa"))
                .with_toggled(st.find.how.match_case)
                .with_on_click(app.clone(), on_match_case as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::MATCH_CASE),
        )
        .with_child(
            Button::create(AzString::from("Word"))
                .with_toggled(st.find.how.whole_word)
                .with_on_click(app.clone(), on_whole_word as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::WHOLE_WORD),
        )
}

/// The editor: the tabs, the bars, the code view of the file in front.
fn editor(app: &RefAny, st: &AppState) -> Dom {
    let Some(doc) = st.tabs.active() else {
        return ShellEmptyState::create(AzString::from("No file open"))
            .with_icon(AzString::from("description"))
            .with_detail(AzString::from("Pick a file in the explorer."))
            .dom()
            .with_id(ids::NO_FILE);
    };
    let labels: Vec<AzString> = st
        .tabs
        .docs
        .iter()
        .map(|d| AzString::from(tab_label(&d.name, d.dirty)))
        .collect();
    let tabs = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center;")
        .with_child(
            Dom::create_div().with_css("flex-grow: 1;").with_child(
                TabHeader::create(labels)
                    .with_active_tab(st.tabs.active)
                    .with_on_click(app.clone(), on_tab_click as TabOnClickCallbackType)
                    .dom()
                    .with_id(ids::TABS),
            ),
        )
        .with_child(
            Button::create(AzString::from("Close"))
                .with_on_click(app.clone(), on_close_tab as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::CLOSE_TAB),
        );
    let mut children = vec![tabs];
    if st.find.open {
        children.push(find_bar(app, st));
    }
    if let Some(typed) = st.goto.as_ref() {
        children.push(goto_bar(app, typed));
    }
    children.push(code_view(app, st, doc));
    column(children)
}

/// The code view over `doc`.
fn code_view(app: &RefAny, st: &AppState, doc: &Doc) -> Dom {
    let (width, height) = st.window;
    CodeView::create(doc.line_count)
        .with_id(ids::EDITOR)
        .with_accessibility_name(AzString::from(doc.name.as_str()))
        .with_view(doc.view.clone())
        .with_tab_width(TAB_WIDTH)
        .with_viewport((width * 0.8).max(200.0), (height - 100.0).max(120.0))
        .with_data_source(doc.text.clone(), doc_line as CodeViewDataSourceCallbackType)
        .with_on_event(app.clone(), on_code_event as CodeViewOnEventCallbackType)
        .dom()
}

/// The find bar: the field, "3 of 14", previous / next, the toggles, and
/// (Mod+H) the replace field with Replace / Replace all.
fn find_bar(app: &RefAny, st: &AppState) -> Dom {
    let count = match (st.find.current, st.find.found.len()) {
        (_, 0) if st.find.query.is_empty() => String::new(),
        (_, 0) => "No results".to_string(),
        (Some(i), n) => format!("{} of {n}", i + 1),
        (None, n) => format!("{n} results"),
    };
    let mut bar = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 4px 6px; flex-wrap: wrap;")
        .with_child(find_input(app, st))
        .with_child(text(&count).with_id(ids::FIND_COUNT))
        .with_child(
            Button::create(AzString::from("Previous"))
                .with_on_click(app.clone(), on_find_previous as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::FIND_PREVIOUS),
        )
        .with_child(
            Button::create(AzString::from("Next"))
                .with_on_click(app.clone(), on_find_next as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::FIND_NEXT),
        )
        .with_child(toggles(app, st));
    if st.find.replace_open {
        bar.add_child(
            TextInput::create()
                .with_text(AzString::from(st.find.replacement.as_str()))
                .with_placeholder(AzString::from("Replace"))
                .with_accessibility_name(AzString::from("Replace"))
                .with_on_text_input(app.clone(), on_replace_text as TextInputOnTextInputCallbackType)
                .dom()
                .with_id(ids::REPLACE_INPUT),
        );
        bar.add_child(
            Button::create(AzString::from("Replace"))
                .with_on_click(app.clone(), on_replace_one as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::REPLACE_ONE),
        );
        bar.add_child(
            Button::create(AzString::from("Replace all"))
                .with_on_click(app.clone(), on_replace_all as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::REPLACE_ALL),
        );
    }
    // TODO(WIDGETS9A): Toolbar - the find bar's buttons as one toolbar.
    bar.with_id(ids::FIND_BAR)
}

/// The go-to-line bar.
fn goto_bar(app: &RefAny, typed: &str) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 4px 6px;")
        .with_child(text("Go to line"))
        .with_child(
            TextInput::create()
                .with_text(AzString::from(typed))
                .with_placeholder(AzString::from("line or line:column"))
                .with_accessibility_name(AzString::from("Go to line"))
                .with_on_text_input(app.clone(), on_goto_text as TextInputOnTextInputCallbackType)
                .with_on_virtual_key_down(app.clone(), on_goto_key as TextInputOnVirtualKeyDownCallbackType)
                .dom()
                .with_id(ids::GOTO_INPUT),
        )
        .with_id(ids::GOTO_BAR)
}

/// The status bar: the caret, the indentation, the encoding, the line
/// endings, the language, the last notice.
fn status_bar(st: &AppState) -> Dom {
    let mut segments = Vec::new();
    if let Some(doc) = st.tabs.active() {
        segments.push(
            StatusBarSegment::create(AzString::from(doc.caret_label(TAB_WIDTH as usize)))
                .with_marker(ids::STATUS_CARET),
        );
        segments.push(StatusBarSegment::create(AzString::from(format!("Spaces: {TAB_WIDTH}"))));
        segments.push(StatusBarSegment::create(AzString::from("UTF-8")));
        segments.push(StatusBarSegment::create(AzString::from(match doc.ending {
            crate::buffer::LineEnding::Lf => "LF",
            crate::buffer::LineEnding::CrLf => "CRLF",
        })));
        segments.push(StatusBarSegment::create(AzString::from(doc.language.as_str())));
    }
    if st.is_saving() {
        segments.push(StatusBarSegment::create(AzString::from("Saving")).with_icon(AzString::from("save")));
    }
    if !st.notice.is_empty() {
        segments.push(StatusBarSegment::create(AzString::from(st.notice.as_str())).with_marker(ids::NOTICE));
    }
    StatusBar::create(segments).dom()
}

// ==== Callbacks ====

/// The CodeView: an edit, a move, undo / redo of the file in front.
pub extern "C" fn on_code_event(mut data: RefAny, mut info: CallbackInfo, event: CodeViewEvent) -> Update {
    with_state(&mut data, &mut info, |st, _info, _app| {
        let Some(doc) = st.tabs.active_mut() else {
            return;
        };
        doc.view = event.view.clone();
        match event.kind {
            CodeViewEventKind::Edit => doc.apply_edits(event.edits.as_slice()),
            CodeViewEventKind::Undo => doc.undo_redo(false),
            CodeViewEventKind::Redo => doc.undo_redo(true),
            _ => return,
        }
        if st.find.open || st.side == Side::Search {
            st.refresh_find();
        }
    })
}

extern "C" fn on_open_sample(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, commands::open_sample)
}

extern "C" fn on_show_explorer(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, _, _| st.side = Side::Explorer)
}

extern "C" fn on_show_search(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, _, _| {
        st.side = Side::Search;
        st.refresh_find();
    })
}

/// A click in the explorer: a file opens, a folder opens or closes.
extern "C" fn on_tree_click(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_state(&mut data, &mut info, |st, info, app| {
        let Some(row) = st.workspace.as_ref().and_then(|w| w.rows().get(index.wrapping_sub(1)).cloned()) else {
            return;
        };
        if let Some(w) = st.workspace.as_mut() {
            w.selected = Some(row.key.clone());
        }
        if row.folder {
            commands::toggle_folder(st, info, app, &row.key, !row.expanded);
        } else {
            commands::open_file(st, info, app, &row.key);
        }
    })
}

/// A folder's arrow.
extern "C" fn on_tree_toggle(mut data: RefAny, mut info: CallbackInfo, index: usize, expanded: bool) -> Update {
    with_state(&mut data, &mut info, |st, info, app| {
        let Some(row) = st.workspace.as_ref().and_then(|w| w.rows().get(index.wrapping_sub(1)).cloned()) else {
            return;
        };
        if row.folder {
            commands::toggle_folder(st, info, app, &row.key, expanded);
        }
    })
}

extern "C" fn on_tab_click(mut data: RefAny, mut info: CallbackInfo, state: TabHeaderState) -> Update {
    with_state(&mut data, &mut info, |st, _, _| {
        if state.active_tab < st.tabs.docs.len() {
            st.tabs.active = state.active_tab;
            st.refresh_find();
        }
    })
}

extern "C" fn on_close_tab(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, _, _| commands::close_tab(st))
}

/// What a search result button selects.
struct MatchRef {
    app: RefAny,
    index: usize,
}

extern "C" fn on_result_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<MatchRef>().map(|m| (m.app.clone(), m.index)) else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |st, _, _| st.select_match(index))
}

/// The find field: the matches follow every key.
extern "C" fn on_find_text(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let query = state.get_text().as_str().to_string();
    let update = with_state(&mut data, &mut info, |st, _, _| {
        st.find.query = query;
        st.refresh_find();
        if let Some(i) = st.find.current {
            st.select_match(i);
        }
        println!("AZCODE_FOUND {}", st.find.found.len());
    });
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// Enter: the next match (Shift: the previous one); Escape closes the bar.
extern "C" fn on_find_key(mut data: RefAny, mut info: CallbackInfo, _state: TextInputState) -> OnTextInputReturn {
    let key = info.get_current_keyboard_state().current_virtual_keycode.into_option();
    let shift = info.get_key_modifiers().shift;
    let update = match key {
        Some(VirtualKeyCode::Return) => {
            info.prevent_default();
            with_state(&mut data, &mut info, |st, _, _| st.step_match(!shift))
        }
        Some(VirtualKeyCode::Escape) => {
            info.prevent_default();
            with_state(&mut data, &mut info, |st, _, _| st.find.open = false)
        }
        _ => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_replace_text(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let replacement = state.get_text().as_str().to_string();
    if let Some(mut st) = data.downcast_mut::<AppState>() {
        st.find.replacement = replacement;
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_find_next(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, _, _| st.step_match(true))
}

extern "C" fn on_find_previous(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, _, _| st.step_match(false))
}

extern "C" fn on_replace_one(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, _, _| {
        if st.replace_current() {
            println!("AZCODE_REPLACED 1");
        }
    })
}

extern "C" fn on_replace_all(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, _, _| {
        let n = st.replace_all();
        st.notice = format!("Replaced {n}");
        println!("AZCODE_REPLACED {n}");
    })
}

extern "C" fn on_match_case(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, _, _| {
        st.find.how.match_case = !st.find.how.match_case;
        st.refresh_find();
    })
}

extern "C" fn on_whole_word(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, _, _| {
        st.find.how.whole_word = !st.find.how.whole_word;
        st.refresh_find();
    })
}

extern "C" fn on_goto_text(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let typed = state.get_text().as_str().to_string();
    if let Some(mut st) = data.downcast_mut::<AppState>() {
        st.goto = Some(typed);
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// Enter goes to the line; Escape closes the bar.
extern "C" fn on_goto_key(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let key = info.get_current_keyboard_state().current_virtual_keycode.into_option();
    let typed = state.get_text().as_str().to_string();
    let update = match key {
        Some(VirtualKeyCode::Return) => {
            info.prevent_default();
            with_state(&mut data, &mut info, |st, _, _| {
                if st.go_to(&typed) {
                    st.goto = None;
                } else {
                    st.notice = format!("\"{typed}\" is not a line");
                }
            })
        }
        Some(VirtualKeyCode::Escape) => {
            info.prevent_default();
            with_state(&mut data, &mut info, |st, _, _| st.goto = None)
        }
        _ => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}
