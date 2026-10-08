//! The window in VSCode's shape, on azul's `OfficeShell` (the S8 developer
//! shell's panes, built here so the side bar can hide - Mod+B - and keep the
//! width its splitter was dragged to):
//!
//! - the ACTIVITY BAR: Explorer and Search (a click on the one in front
//!   hides the side bar), Settings at its foot;
//! - the SIDE BAR: the explorer ([`crate::explorer`]: the folder's tree,
//!   virtualized, or "You have not yet opened a folder." with Open Folder and
//!   the recent folders) or the search over the folder
//!   ([`crate::find_in_files`]);
//! - the EDITOR: the tabs (a close button each, a dot for unsaved changes),
//!   the file's path, the find and go-to bars, azul's CodeView over the file
//!   in front - or, while no file is open, the welcome page (the app's name,
//!   Start, Recent, the keyboard shortcuts); under it, on a splitter, the
//!   TERMINAL panel ([`crate::terminal`]);
//! - the STATUS BAR (the branch, the folder, Ln / Col, the indentation, the
//!   encoding, the line endings, the language, the last notice), and the
//!   palette (quick open, the command palette: [`crate::palette`]) over it
//!   all.
//!
//! The chrome AzCode draws itself paints with the theme's ink and the
//! accent (`--az-accent`) and greys that read in either mode, so it follows
//! the app theme and the mode like the widgets around it.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CallbackType, CodeViewDataSourceCallbackType,
        CodeViewOnEventCallbackType, ShellOnPaneResizeCallbackType, SplitPaneOnResizeCallbackType,
        TextInputOnTextInputCallbackType, TextInputOnVirtualKeyDownCallbackType,
        ToolbarOnEventCallbackType,
    },
    dom::VirtualKeyCode,
    prelude::*,
    shells::{OfficeShell, ShellPane, ShellPaneKind},
    str::String as AzString,
    widgets::{
        CodeView, CodeViewEvent, CodeViewEventKind, OnTextInputReturn, SplitDirection, SplitPane,
        SplitPaneState, StatusBar, StatusBarSegment, TextInput, TextInputState, TextInputValid,
        Toolbar, ToolbarEvent, ToolbarEventKind, ToolbarItem,
    },
};
use azul_appkit::ui as kit;

use crate::{
    actions::{self, Action},
    app::{doc_line, AppState, Doc, Side},
    commands, explorer, find_in_files, ids, palette, terminal,
};

/// The columns between two tab stops.
pub const TAB_WIDTH: u32 = 4;
/// The activity bar's width in px (VSCode's).
const ACTIVITY_WIDTH: f32 = 48.0;
/// The panes' DOM ids (the S8 developer shell's).
const ACTIVITY_BAR_ID: &str = "shell-activity-bar";
const SIDE_BAR_ID: &str = "shell-side-bar";
const EDITOR_ID: &str = "shell-editor";
/// The hairline between parts of the chrome: grey, so it reads in either
/// mode.
pub const RULE: &str = "rgba(128, 128, 128, 0.25)";
/// The face of what is not in front (a tab behind, the strip after the
/// tabs): the ground, a shade darker in light mode, lighter in dark mode.
const RECESSED: &str = "rgba(128, 128, 128, 0.10)";

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

/// A line of text in a row (the find bar's count).
fn text(content: &str) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 0px 6px;")
        .with_child(Dom::create_span_with_text(content))
}

/// A run of text in a box styled with `css`.
pub fn label(content: &str, css: &str) -> Dom {
    Dom::create_div()
        .with_css(css)
        .with_child(Dom::create_span_with_text(content))
}

/// A column that fills its parent.
pub fn column(children: Vec<Dom>) -> Dom {
    let mut c = Dom::create_div().with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;");
    for child in children {
        c.add_child(child);
    }
    c
}

/// A clickable box: `callback(data)` on a click (the pointer coming up over
/// it), named `name` for a screen reader.
pub fn clickable(id: AzString, css: &str, name: &str, data: RefAny, callback: CallbackType) -> Dom {
    Dom::create_div()
        .with_id(id)
        .with_css(css)
        .with_accessibility_name(name)
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), data, callback)
}

// ==== The window ====

/// The whole window's content (under the theme scope): the shell, and the
/// palette over it while it is showing.
pub fn window(app: &RefAny, st: &AppState) -> Dom {
    let (side, side_label) = match st.side {
        Side::Explorer => (explorer::explorer(app, st), "Explorer"),
        Side::Search => (find_in_files::search_panel(app, st), "Search"),
    };
    let shell = OfficeShell::create()
        .with_pane(
            ShellPane::create(ACTIVITY_BAR_ID, activity_bar(app, st))
                .with_kind(ShellPaneKind::Navigation)
                .with_label("Activity bar")
                .with_width(ACTIVITY_WIDTH),
        )
        .with_pane(
            ShellPane::create(SIDE_BAR_ID, side)
                .with_kind(ShellPaneKind::Navigation)
                .with_label(side_label)
                .with_ratio(st.side_ratio)
                .with_visible(st.side_visible),
        )
        .with_pane(
            ShellPane::create(EDITOR_ID, editor_area(app, st))
                .with_kind(ShellPaneKind::Main)
                .with_label("Editor"),
        )
        .with_title_row(kit::title_row(&st.title()))
        .with_status_bar(status_bar(app, st))
        .with_on_pane_resize(app.clone(), on_pane_resize as ShellOnPaneResizeCallbackType);
    // Positioned: the palette's backdrop covers the window from here.
    let mut root = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; position: relative;",
        )
        .with_child(shell.dom());
    if let Some(overlay) = palette::palette(app, st) {
        root.add_child(overlay);
    }
    root
}

/// The editor and, while it is open, the terminal panel under it on a
/// splitter.
fn editor_area(app: &RefAny, st: &AppState) -> Dom {
    let editor = editor(app, st);
    if !st.panel.open {
        return editor;
    }
    SplitPane::create(SplitDirection::Vertical, editor, terminal::panel(app, st))
        .with_ratio(st.panel.editor_ratio)
        .with_on_resize(app.clone(), on_panel_resize as SplitPaneOnResizeCallbackType)
        .dom()
}

/// The editor's box, the CodeView's viewport hint: the window less the
/// activity bar, the side bar, the title row, the status bar, the terminal
/// panel, the tabs, the path and the open bars (a little too much is fine:
/// the view clips, and measures its real box at every action).
#[must_use]
pub fn editor_size(st: &AppState) -> (f32, f32) {
    let (width, height) = st.window;
    let rest = (width - ACTIVITY_WIDTH).max(0.0);
    let side = if st.side_visible {
        rest * st.side_ratio + 4.0
    } else {
        0.0
    };
    // The title row and the status bar span the window.
    let mut body = height - 30.0 - 24.0;
    if st.panel.open {
        body = body * st.panel.editor_ratio - 4.0;
    }
    // The tabs, the path.
    let mut chrome = 35.0 + 22.0;
    if st.find.open {
        chrome += 36.0;
    }
    if st.goto.is_some() {
        chrome += 32.0;
    }
    ((rest - side).max(200.0), (body - chrome).max(120.0))
}

// ==== The activity bar ====

/// Explorer and Search at the top, Settings at the foot.
fn activity_bar(app: &RefAny, st: &AppState) -> Dom {
    let showing = |side: Side| st.side_visible && st.side == side;
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(activity_item(
            app,
            ids::ACTIVITY_EXPLORER,
            "content_copy",
            "Explorer",
            showing(Side::Explorer),
            on_show_explorer,
        ))
        .with_child(activity_item(
            app,
            ids::ACTIVITY_SEARCH,
            "search",
            "Search",
            showing(Side::Search),
            on_show_search,
        ))
        .with_child(Dom::create_div().with_css("flex-grow: 1;"))
        .with_child(activity_item(
            app,
            ids::ACTIVITY_SETTINGS,
            "settings",
            "Settings",
            false,
            on_open_settings,
        ))
}

/// One icon of the activity bar: the one in front has the accent's bar on
/// its left and full ink, the others are dimmed until hovered.
fn activity_item(
    app: &RefAny,
    id: AzString,
    icon: &str,
    name: &str,
    active: bool,
    callback: CallbackType,
) -> Dom {
    let look = if active {
        "border-left: 2px solid var(--az-accent, #0078D4);"
    } else {
        "border-left: 2px solid transparent; opacity: 0.55; :hover { opacity: 1; }"
    };
    clickable(
        id,
        &format!(
            "display: flex; flex-direction: row; align-items: center; justify-content: center; \
             height: 48px; flex-shrink: 0; cursor: pointer; {look}"
        ),
        name,
        app.clone(),
        callback,
    )
    .with_child(Dom::create_icon(icon).with_css("font-size: 24px;"))
}

// ==== The side bar ====

/// The side bar's title row ("EXPLORER") with its actions at the right.
pub fn side_title(title: &str, actions: Vec<Dom>) -> Dom {
    let mut row = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; align-items: center; height: 35px; flex-shrink: 0; \
             padding: 0px 8px 0px 20px;",
        )
        .with_child(label(title, "flex-grow: 1; font-size: 11px; opacity: 0.8;"));
    for action in actions {
        row.add_child(action);
    }
    row
}

/// A small icon button of a title row.
pub fn icon_button(id: AzString, icon: &str, name: &str, data: RefAny, callback: CallbackType) -> Dom {
    clickable(
        id,
        "display: flex; flex-direction: row; align-items: center; justify-content: center; \
         width: 22px; height: 22px; border-radius: 4px; cursor: pointer; opacity: 0.8; \
         :hover { background: rgba(128, 128, 128, 0.25); opacity: 1; }",
        name,
        data,
        callback,
    )
    .with_child(Dom::create_icon(icon).with_css("font-size: 16px;"))
}

/// Recent folder `index`: its name, the folder it is in; a click opens it.
pub fn recent_row(app: &RefAny, id: AzString, index: usize, folder: &str) -> Dom {
    let path = std::path::Path::new(folder);
    let name = path
        .file_name()
        .map_or_else(|| folder.to_string(), |n| n.to_string_lossy().into_owned());
    let parent = path.parent().map(|p| p.display().to_string()).unwrap_or_default();
    clickable(
        id,
        "display: flex; flex-direction: row; align-items: center; padding: 3px 4px; border-radius: 3px; \
         cursor: pointer; :hover { background: rgba(128, 128, 128, 0.15); }",
        &format!("Open {folder}"),
        RefAny::new(RecentRef { app: app.clone(), index }),
        on_recent_click,
    )
    .with_child(Dom::create_icon("folder").with_css("font-size: 16px; padding-right: 6px; opacity: 0.8;"))
    .with_child(label(
        &name,
        "font-size: 13px; color: var(--az-accent, #3794FF); padding-right: 8px; flex-shrink: 0;",
    ))
    .with_child(label(
        &parent,
        "font-size: 12px; opacity: 0.6; white-space: nowrap; overflow: hidden; min-width: 0px;",
    ))
}

/// A find field (the find bar's).
fn find_input(app: &RefAny, st: &AppState, id: AzString) -> Dom {
    TextInput::create()
        .with_text(AzString::from(st.find.query.as_str()))
        .with_placeholder(AzString::from("Find"))
        .with_accessibility_name(AzString::from("Find"))
        .with_on_text_input(app.clone(), on_find_text as TextInputOnTextInputCallbackType)
        .with_on_virtual_key_down(app.clone(), on_find_key as TextInputOnVirtualKeyDownCallbackType)
        .dom()
        .with_id(id)
}

// ==== The editor ====

/// The editor: the tabs, the path, the bars, the code view of the file in
/// front; the welcome page while no file is open.
fn editor(app: &RefAny, st: &AppState) -> Dom {
    let Some(doc) = st.tabs.active() else {
        return welcome(app, st);
    };
    let mut children = vec![tab_strip(app, st), breadcrumbs(doc)];
    if st.find.open {
        children.push(find_bar(app, st));
    }
    if let Some(typed) = st.goto.as_ref() {
        children.push(goto_bar(app, typed));
    }
    children.push(code_view(app, st, doc));
    column(children)
}

/// The tabs, VSCode's way: the file's name and a close button each (a dot
/// while it has unsaved changes), the one in front on the editor's ground
/// under the accent's line, the others recessed.
fn tab_strip(app: &RefAny, st: &AppState) -> Dom {
    let mut strip = Dom::create_div().with_id(ids::TABS).with_css(
        "display: flex; flex-direction: row; align-items: stretch; height: 35px; flex-shrink: 0; \
         min-width: 0px; overflow: hidden;",
    );
    for (i, doc) in st.tabs.docs.iter().enumerate() {
        strip.add_child(tab(app, i, doc, i == st.tabs.active));
    }
    strip.add_child(Dom::create_div().with_css(format!(
        "flex-grow: 1; background: {RECESSED}; border-bottom: 1px solid {RULE};"
    )));
    strip
}

/// Tab `index` (`doc`): its name (a click brings it to front) and its close
/// button (the dot of unsaved changes in its place).
fn tab(app: &RefAny, index: usize, doc: &Doc, active: bool) -> Dom {
    let face = if active {
        "border-top: 1px solid var(--az-accent, #0078D4); border-bottom: 1px solid transparent;".to_string()
    } else {
        format!("background: {RECESSED}; border-top: 1px solid transparent; border-bottom: 1px solid {RULE};")
    };
    let tab_ref = || RefAny::new(TabRef { app: app.clone(), index });
    let name = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; align-items: center; padding: 0px 4px 0px 12px; \
             cursor: pointer;",
        )
        .with_accessibility_name(doc.name.as_str())
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), tab_ref(), on_tab_click)
        .with_child(
            Dom::create_icon("description").with_css("font-size: 16px; padding-right: 6px; opacity: 0.8;"),
        )
        .with_child(label(
            &doc.name,
            if active {
                "font-size: 13px; white-space: nowrap;"
            } else {
                "font-size: 13px; white-space: nowrap; opacity: 0.7;"
            },
        ));
    // The dot is a plain box, not an icon: icon resolution replaces an icon
    // node whole - its classes too - so a class on `Dom::create_icon` never
    // reached the window.
    let glyph = if doc.dirty {
        Dom::create_div()
            .with_class(ids::TAB_DIRTY_CLASS)
            .with_accessibility_name("Unsaved changes")
            .with_css("width: 8px; height: 8px; border-radius: 4px; background: system:text; opacity: 0.85;")
    } else if active {
        Dom::create_icon("close").with_css("font-size: 16px;")
    } else {
        Dom::create_icon("close").with_css("font-size: 16px; opacity: 0.5;")
    };
    let close = clickable(
        ids::tab_close(index),
        "display: flex; flex-direction: row; align-items: center; justify-content: center; width: 20px; \
         height: 20px; margin-right: 6px; border-radius: 4px; cursor: pointer; \
         :hover { background: rgba(128, 128, 128, 0.25); }",
        &format!("Close {}", doc.name),
        tab_ref(),
        on_tab_close,
    )
    .with_child(glyph);
    let mut tab = Dom::create_div()
        .with_id(ids::tab(index))
        .with_class(ids::TAB_CLASS)
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; \
             border-right: 1px solid {RULE}; {face}"
        ))
        .with_child(name)
        .with_child(close);
    if active {
        tab = tab.with_class(ids::TAB_ACTIVE_CLASS);
    }
    tab
}

/// The path of the file in front, under the tabs (`src › main.rs`).
fn breadcrumbs(doc: &Doc) -> Dom {
    let path = doc
        .key
        .split('/')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("  \u{203A}  ");
    Dom::create_div()
        .with_id(ids::BREADCRUMBS)
        .with_css(
            "display: flex; flex-direction: row; align-items: center; height: 22px; flex-shrink: 0; \
             padding: 0px 12px; font-size: 12px; opacity: 0.7;",
        )
        .with_child(Dom::create_span_with_text(path))
}

/// The code view over `doc`: it builds only the lines in its viewport (a
/// million-line file costs a screen).
fn code_view(app: &RefAny, st: &AppState, doc: &Doc) -> Dom {
    let (width, height) = editor_size(st);
    CodeView::create(doc.line_count)
        .with_id(ids::EDITOR)
        .with_accessibility_name(AzString::from(doc.name.as_str()))
        .with_view(doc.view.clone())
        .with_tab_width(TAB_WIDTH)
        .with_viewport(width, height)
        .with_data_source(doc.text.clone(), doc_line as CodeViewDataSourceCallbackType)
        .with_on_event(app.clone(), on_code_event as CodeViewOnEventCallbackType)
        .dom()
}

/// What the welcome page lists under "Keyboard shortcuts".
const WELCOME_KEYS: [(&str, &str); 12] = [
    ("Open Folder", "Mod+K Mod+O"),
    ("Quick Open a File", "Mod+P"),
    ("Command Palette", "Mod+Shift+P"),
    ("Find in Files", "Mod+Shift+F"),
    ("Toggle Terminal", "Ctrl+`"),
    ("Save", "Mod+S"),
    ("Close the Tab", "Mod+W"),
    ("Show / Hide the Side Bar", "Mod+B"),
    ("Find in the File", "Mod+F"),
    ("Go to Line", "Mod+G"),
    ("Settings", "Mod+,"),
    ("Every Shortcut", "F1"),
];

/// The editor while no file is open: VSCode's welcome page - the app's
/// name, Start (Open Folder..., Open File..., the sample), Recent, the
/// keyboard shortcuts.
fn welcome(app: &RefAny, st: &AppState) -> Dom {
    let mut page = Dom::create_div()
        .with_css("display: flex; flex-direction: column; width: 560px; max-width: 100%; padding: 48px 32px;")
        .with_child(label("AzCode", "font-size: 36px; font-weight: 300;"))
        .with_child(label("A code editor on azul", "font-size: 16px; opacity: 0.7; padding-bottom: 12px;"))
        .with_child(heading("Start"))
        .with_child(link(
            app,
            ids::WELCOME_OPEN_FOLDER,
            "create_new_folder",
            "Open Folder...",
            &commands::keys("Mod+K Mod+O"),
            on_open_folder,
        ))
        .with_child(link(app, ids::WELCOME_OPEN_FILE, "file_open", "Open File...", "", on_open_file))
        .with_child(link(
            app,
            ids::OPEN_SAMPLE,
            "science",
            "Open the sample workspace",
            "",
            on_open_sample,
        ));
    if !st.recent.is_empty() {
        page.add_child(heading("Recent"));
        for (i, folder) in st.recent.iter().enumerate().take(5) {
            page.add_child(recent_row(app, ids::welcome_recent(i), i, folder));
        }
    }
    page.add_child(heading("Keyboard shortcuts"));
    for (action, keys) in WELCOME_KEYS {
        page.add_child(shortcut_row(action, &commands::keys(keys)));
    }
    Dom::create_div()
        .with_id(ids::WELCOME)
        .with_css(
            "display: flex; flex-direction: column; align-items: center; flex-grow: 1; min-height: 0px; \
             overflow-y: auto;",
        )
        .with_child(page)
}

/// A heading of the welcome page.
fn heading(title: &str) -> Dom {
    label(title, "font-size: 15px; font-weight: 600; padding: 20px 0px 8px 0px;")
}

/// A link of the welcome page: its icon, its title in the accent, its keys.
fn link(
    app: &RefAny,
    id: AzString,
    icon: &str,
    title: &str,
    keys: &str,
    callback: CallbackType,
) -> Dom {
    let mut row = clickable(
        id,
        "display: flex; flex-direction: row; align-items: center; padding: 4px 0px; cursor: pointer; \
         color: var(--az-accent, #3794FF); :hover { opacity: 0.8; }",
        title,
        app.clone(),
        callback,
    )
    .with_child(Dom::create_icon(icon).with_css("font-size: 18px; padding-right: 8px;"))
    .with_child(label(title, "font-size: 13px;"));
    if !keys.is_empty() {
        row.add_child(label(keys, "font-size: 12px; opacity: 0.6; padding-left: 10px;"));
    }
    row
}

/// A row of the welcome page's shortcuts: what, then the keys in a box.
fn shortcut_row(action: &str, keys: &str) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 3px 0px;")
        .with_child(label(action, "font-size: 13px; opacity: 0.8; width: 240px; flex-shrink: 0;"))
        .with_child(label(
            keys,
            &format!("font-size: 12px; padding: 1px 6px; border: 1px solid {RULE}; border-radius: 3px;"),
        ))
}

/// The find bar: the field, "3 of 14", previous / next, the toggles, and
/// (Mod+H) the replace field with Replace / Replace all - azul's `Toolbar`
/// (the fields embedded, never in the "more" menu). Each tool's `id` is its
/// DOM-id name from [`ids`]: what [`on_find_tool`] matches.
fn find_bar(app: &RefAny, st: &AppState) -> Dom {
    let count = match (st.find.current, st.find.found.len()) {
        (_, 0) if st.find.query.is_empty() => String::new(),
        (_, 0) => "No results".to_string(),
        (Some(i), n) => format!("{} of {n}", i + 1),
        (None, n) => format!("{n} results"),
    };
    let mut items = vec![
        ToolbarItem::create_custom(ids::FIND_INPUT, "Find", find_input(app, st, ids::FIND_INPUT), 220.0)
            .with_never_overflow(true),
        ToolbarItem::create_custom(ids::FIND_COUNT, "Results", text(&count).with_id(ids::FIND_COUNT), 90.0),
        ToolbarItem::create_button(ids::FIND_PREVIOUS, "Previous", ""),
        ToolbarItem::create_button(ids::FIND_NEXT, "Next", ""),
        ToolbarItem::create_separator(),
        ToolbarItem::create_toggle(ids::MATCH_CASE, "Aa", "", st.find.how.match_case).with_tooltip("Match case"),
        ToolbarItem::create_toggle(ids::WHOLE_WORD, "Word", "", st.find.how.whole_word).with_tooltip("Whole word"),
    ];
    if st.find.replace_open {
        let replace = TextInput::create()
            .with_text(AzString::from(st.find.replacement.as_str()))
            .with_placeholder(AzString::from("Replace"))
            .with_accessibility_name(AzString::from("Replace"))
            .with_on_text_input(app.clone(), on_replace_text as TextInputOnTextInputCallbackType)
            .dom()
            .with_id(ids::REPLACE_INPUT);
        items.push(ToolbarItem::create_separator());
        items.push(ToolbarItem::create_custom(ids::REPLACE_INPUT, "Replace with", replace, 220.0).with_never_overflow(true));
        items.push(ToolbarItem::create_button(ids::REPLACE_ONE, "Replace", ""));
        items.push(ToolbarItem::create_button(ids::REPLACE_ALL, "Replace all", ""));
    }
    Toolbar::create("Find")
        .with_items(items)
        .with_available_width(editor_size(st).0)
        .with_on_event(app.clone(), on_find_tool as ToolbarOnEventCallbackType)
        .dom()
        .with_id(ids::FIND_BAR)
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

/// The status bar: the branch, the folder, the terminal, the caret (a click
/// goes to a line), the indentation, the encoding, the line endings, the
/// language, the last notice.
fn status_bar(app: &RefAny, st: &AppState) -> Dom {
    let mut segments = Vec::new();
    if let Some(branch) = st.branch.as_ref() {
        segments.push(
            StatusBarSegment::create(AzString::from(branch.as_str()))
                .with_icon(AzString::from("call_split"))
                .with_marker(ids::STATUS_BRANCH),
        );
    }
    if let Some(w) = st.workspace.as_ref() {
        segments.push(
            StatusBarSegment::create(AzString::from(w.root.name.as_str())).with_icon(AzString::from("folder_open")),
        );
    }
    let shells = st.panel.terminals.len();
    segments.push(
        StatusBarSegment::create(AzString::from(if shells == 0 {
            "Terminal".to_string()
        } else {
            format!("Terminal ({shells})")
        }))
        .with_icon(AzString::from("terminal"))
        .with_on_click(app.clone(), on_status_terminal as ButtonOnClickCallbackType),
    );
    if let Some(doc) = st.tabs.active() {
        segments.push(
            StatusBarSegment::create(AzString::from(doc.caret_label(TAB_WIDTH as usize)))
                .with_marker(ids::STATUS_CARET)
                .with_on_click(app.clone(), on_status_caret as ButtonOnClickCallbackType),
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
    // A scroll moves only the lines, and the view has rendered them again
    // itself: the view is kept for the next build, the window is not
    // rebuilt. Rebuilding it for every wheel notch (the layout callback, the
    // cascade and the layout of the whole workbench) was why scrolling the
    // code lagged.
    if event.kind == CodeViewEventKind::Scroll {
        if let Some(mut st) = data.downcast_mut::<AppState>() {
            if let Some(doc) = st.tabs.active_mut() {
                doc.view = event.view;
            }
        }
        return Update::DoNothing;
    }
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
        if st.find.open {
            st.refresh_find();
        }
    })
}

/// The side bar's splitter moved: its share is kept for the next build.
extern "C" fn on_pane_resize(mut data: RefAny, _info: CallbackInfo, pane: usize, ratio: f32) -> Update {
    // Pane 1 is the side bar (0 is the activity bar, a rail outside the splits).
    if pane == 1 {
        if let Some(mut st) = data.downcast_mut::<AppState>() {
            st.side_ratio = ratio.clamp(0.08, 0.6);
        }
    }
    Update::DoNothing
}

/// The splitter between the editor and the terminal panel moved.
extern "C" fn on_panel_resize(mut data: RefAny, _info: CallbackInfo, state: SplitPaneState) -> Update {
    if let Some(mut st) = data.downcast_mut::<AppState>() {
        st.panel.editor_ratio = state.ratio.clamp(0.15, 0.9);
    }
    Update::DoNothing
}

/// The status bar's Terminal: the panel opens or closes.
extern "C" fn on_status_terminal(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, info, app| {
        actions::run(st, info, app, Action::ToggleTerminal);
    })
}

/// The status bar's "Ln, Col": go to line.
extern "C" fn on_status_caret(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, info, app| actions::run(st, info, app, Action::GoToLine))
}

/// "Open Folder" (the empty explorer, the welcome page).
pub extern "C" fn on_open_folder(data: RefAny, _info: CallbackInfo) -> Update {
    commands::ask_folder(&data);
    Update::DoNothing
}

extern "C" fn on_open_file(data: RefAny, _info: CallbackInfo) -> Update {
    commands::ask_file(&data);
    Update::DoNothing
}

extern "C" fn on_open_sample(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, commands::open_sample)
}

extern "C" fn on_open_settings(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(kit_ref) = data.downcast_ref::<AppState>().map(|s| s.kit.clone()) else {
        return Update::DoNothing;
    };
    kit::open_settings(&kit_ref, None);
    Update::RefreshDom
}

/// An activity icon: its panel in the side bar, or - the one in front - the
/// side bar hidden (VSCode's way).
fn show_side(st: &mut AppState, info: &mut CallbackInfo, side: Side) {
    if st.side_visible && st.side == side {
        st.side_visible = false;
        return;
    }
    st.side = side;
    st.side_visible = true;
    if side == Side::Search {
        commands::focus_soon(info, ids::SEARCH_INPUT.as_str());
    }
}

extern "C" fn on_show_explorer(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, info, _| show_side(st, info, Side::Explorer))
}

extern "C" fn on_show_search(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, info, _| show_side(st, info, Side::Search))
}

/// What a recent folder's row opens.
struct RecentRef {
    app: RefAny,
    index: usize,
}

extern "C" fn on_recent_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<RecentRef>().map(|r| (r.app.clone(), r.index)) else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |st, info, app| commands::open_recent(st, info, app, index))
}

/// What a tab's name and its close button act on.
struct TabRef {
    app: RefAny,
    index: usize,
}

fn tab_of(data: &mut RefAny) -> Option<(RefAny, usize)> {
    data.downcast_ref::<TabRef>().map(|t| (t.app.clone(), t.index))
}

/// A tab's name: the tab to front, the editor focused.
extern "C" fn on_tab_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = tab_of(&mut data) else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |st, info, _| {
        if index < st.tabs.docs.len() {
            st.tabs.active = index;
            st.refresh_find();
            commands::focus_soon(info, ids::EDITOR.as_str());
        }
    })
}

/// A tab's close button.
extern "C" fn on_tab_close(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = tab_of(&mut data) else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |st, _, _| commands::close_tab_at(st, index))
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

/// Enter: the next match (Shift: the previous one); Escape closes the bar
/// and gives the editor the focus back.
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
            with_state(&mut data, &mut info, |st, info, _| {
                st.find.open = false;
                commands::focus_soon(info, ids::EDITOR.as_str());
            })
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

/// A tool of the find bar: previous / next, replace / replace all, the
/// match-case and whole-word toggles (the event carries the new state).
extern "C" fn on_find_tool(mut data: RefAny, mut info: CallbackInfo, event: ToolbarEvent) -> Update {
    let id = event.id.as_str();
    match event.kind {
        ToolbarEventKind::Toggle => with_state(&mut data, &mut info, |st, _, _| {
            if id == ids::MATCH_CASE.as_str() {
                st.find.how.match_case = event.pressed;
            } else if id == ids::WHOLE_WORD.as_str() {
                st.find.how.whole_word = event.pressed;
            } else {
                return;
            }
            st.refresh_find();
        }),
        ToolbarEventKind::Activate => {
            if id == ids::FIND_PREVIOUS.as_str() {
                on_find_previous(data, info)
            } else if id == ids::FIND_NEXT.as_str() {
                on_find_next(data, info)
            } else if id == ids::REPLACE_ONE.as_str() {
                on_replace_one(data, info)
            } else if id == ids::REPLACE_ALL.as_str() {
                on_replace_all(data, info)
            } else {
                Update::DoNothing
            }
        }
        ToolbarEventKind::Choose => Update::DoNothing,
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

/// Enter goes to the line; Escape closes the bar. Either way the editor
/// gets the focus back.
extern "C" fn on_goto_key(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let key = info.get_current_keyboard_state().current_virtual_keycode.into_option();
    let typed = state.get_text().as_str().to_string();
    let update = match key {
        Some(VirtualKeyCode::Return) => {
            info.prevent_default();
            with_state(&mut data, &mut info, |st, info, _| {
                if st.go_to(&typed) {
                    st.goto = None;
                    commands::focus_soon(info, ids::EDITOR.as_str());
                } else {
                    st.notice = format!("\"{typed}\" is not a line");
                }
            })
        }
        Some(VirtualKeyCode::Escape) => {
            info.prevent_default();
            with_state(&mut data, &mut info, |st, info, _| {
                st.goto = None;
                commands::focus_soon(info, ids::EDITOR.as_str());
            })
        }
        _ => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}
