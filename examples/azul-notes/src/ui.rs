//! The window: the app-drawn title row (the window is `NoTitle`), then the
//! notes in the three-pane PIM shell (S4) - navigation, the note list, the
//! editor - or the settings or a note's history, with the sheets (the
//! command palette, a notebook's name, a link, a confirmation) over it.
//! Every callback of the chrome lives here; the editor's own are in
//! `editor.rs`, the storage answers in `jobs.rs`.

use std::path::PathBuf;

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CallbackInfo, ChipOnRemoveCallbackType, LayoutCallbackInfo,
        MessageListOnEventCallbackType, RefAny, SegmentedOnChangeCallbackType,
        ShellCommandPaletteOnQueryCallbackType, ShellCommandPaletteOnRunCallbackType,
        ShellNavigationPaneOnEventCallbackType, ShellOnPaneResizeCallbackType,
        ShellSettingsLayoutOnCategoryCallbackType, ShellSettingsLayoutOnSearchCallbackType,
        TextInputOnFocusLostCallbackType, TextInputOnTextInputCallbackType,
        TextInputOnVirtualKeyDownCallbackType, Update,
    },
    css::{ColorU, DarkLightMode, EventFilter},
    dom::{Dom, VirtualKeyCode},
    option::OptionDarkLightMode,
    shells::{
        PimShell, ShellCommandPalette, ShellEmptyState, ShellNavigationGroup,
        ShellNavigationPane, ShellNavigationPaneEvent, ShellNavigationPaneEventKind,
        ShellPaletteCommand, ShellSettingsLayout, ShellSettingsSection, ShellThemeAccent,
        ShellThemeScope,
    },
    str::String as AzString,
    vec::{DomVec, MessageRowVec, StringVec},
    widgets::{
        Button, ButtonType, Chip, ChipState, MessageList, MessageListEvent,
        MessageListEventKind, MessageListMark, MessageRow, OnTextInputReturn, Segmented,
        SegmentedState, StatusBar, StatusBarSegment, StatusBarSync, StatusBarSyncKind,
        TextInput, TextInputState, TextInputValid, Titlebar, TreeViewNode,
    },
    window::WindowEventFilter,
};

use crate::{
    doc::{BlockKind, Format},
    editor, jobs,
    look::{self, Look, TextSize},
    model::{self, ListRow, NotebookNode, Scope, SortKey},
    store::Job,
    with_state, AppState, HistoryView, Overlay, Screen, Status,
};

/// The settings' categories.
pub const SETTINGS_CATEGORIES: [&str; 5] = ["General", "Editor", "Storage", "Keyboard shortcuts", "About"];
pub const SETTINGS_EDITOR: usize = 1;
pub const SETTINGS_STORAGE: usize = 2;
pub const SETTINGS_SHORTCUTS: usize = 3;
pub const SETTINGS_ABOUT: usize = 4;

/// The keyboard shortcuts, as the settings list them.
pub const SHORTCUTS: &[(&str, &str)] = &[
    ("New note", "Ctrl/Cmd+N"),
    ("Command palette", "Ctrl/Cmd+K"),
    ("Save now (and keep a version)", "Ctrl/Cmd+S"),
    ("Pin or unpin the note", "Ctrl/Cmd+Shift+P"),
    ("Version history", "Ctrl/Cmd+Shift+H"),
    ("Settings", "Ctrl/Cmd+,"),
    ("Close a sheet, leave settings", "Escape"),
    ("Bold / italic / underline", "Ctrl/Cmd+B / I / U"),
    ("Strikethrough", "Ctrl/Cmd+Shift+X"),
    ("Inline code", "Ctrl/Cmd+E"),
    ("Link", "Ctrl/Cmd+Shift+K"),
    ("Paragraph / heading 1-3", "Ctrl/Cmd+0 / 1 / 2 / 3"),
    ("Numbered / bulleted / check list", "Ctrl/Cmd+Shift+7 / 8 / 9"),
    ("Tick a check item", "Ctrl/Cmd+Enter"),
    ("Indent / outdent a list item", "Tab / Shift+Tab"),
    ("Heading, list, check item, quote, code", "Type # , - , 1. , [ ] , > , ``` at a line's start"),
    ("Next / previous pane", "F6 / Shift+F6"),
];

fn az(s: impl Into<String>) -> AzString {
    AzString::from(s.into())
}

fn strs(items: &[&str]) -> StringVec {
    StringVec::from_vec(items.iter().map(|s| AzString::from(*s)).collect())
}

/// A line of text.
fn text_line(text: &str, css: &str) -> Dom {
    Dom::create_p_with_text(text).with_css(format!("margin: 0px; {css}"))
}

// ==== The window ====

/// The window's layout.
pub extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let look = look::of(matches!(info.get_mode(), DarkLightMode::Dark));
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<AppState>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    let content = match s.screen {
        Screen::Notes => notes_screen(s, &app, look),
        Screen::Settings => settings_screen(s, &app, look),
        Screen::History => history_screen(s, &app, look),
    };
    let area = Dom::create_div()
        .with_id("notes-area")
        .with_css("position: relative; display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(content)
        .with_child(overlay_dom(s, &app, look));
    let column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(title_row(look))
        .with_child(area);
    Dom::create_body()
        .with_css("display: flex; flex-direction: column; height: 100%; margin: 0px; font-family: sans-serif;")
        .with_child(
            ShellThemeScope::create(column)
                .with_accent(ShellThemeAccent::Leaf)
                .dom(),
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            app.clone(),
            on_window_key,
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::WindowFocusReceived),
            app.clone(),
            jobs::on_window_focus,
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::CloseRequested),
            app.clone(),
            jobs::on_close_requested,
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::DroppedFile),
            app,
            on_dropped_file,
        )
}

/// The window's title row, drawn by azul (macOS draws only the traffic
/// lights): the chrome's colour, no line of its own.
fn title_row(look: &Look) -> Dom {
    let (r, g, b) = look.chrome_rgb;
    let mut bar = Titlebar::create("AzNotes")
        .with_background(ColorU::rgb(r, g, b))
        .without_border_bottom();
    if look.dark {
        let (r, g, b) = look.text_rgb;
        bar.title_color = ColorU::rgb(r, g, b);
    }
    bar.dom()
}

/// The notes: the PIM shell, or a placeholder while loading.
fn notes_screen(s: &AppState, app: &RefAny, look: &Look) -> Dom {
    if !s.loaded {
        return ShellEmptyState::create("Loading notes")
            .with_icon("hourglass_empty")
            .with_detail(s.root.display().to_string())
            .dom();
    }
    PimShell::create(navigation_pane(s, app), note_list(s, app), reading_pane(s, app, look))
        .with_status_bar(status_bar(s))
        .with_list_label("Notes")
        .with_navigation_ratio(s.nav.navigation_ratio)
        .with_list_ratio(s.nav.list_ratio)
        .with_on_pane_resize(app.clone(), on_pane_resize as ShellOnPaneResizeCallbackType)
        .dom()
}

/// The status bar: how many notes, the open note's words, the save state.
fn status_bar(s: &AppState) -> Dom {
    let counts = s.library.counts();
    let mut segments = vec![StatusBarSegment::create(format!(
        "{} note{}",
        counts.all,
        if counts.all == 1 { "" } else { "s" }
    ))];
    if let Some(note) = s.open_note() {
        segments.push(StatusBarSegment::create(model::status_text(note)));
    }
    let (label, kind) = match &s.status {
        Status::Saving => ("Saving...".to_string(), StatusBarSyncKind::Syncing),
        Status::Editing => ("Edited".to_string(), StatusBarSyncKind::Syncing),
        Status::Error(_) => (s.status.label(), StatusBarSyncKind::Error),
        Status::Idle | Status::Saved => ("Saved".to_string(), StatusBarSyncKind::Connected),
    };
    StatusBar::create(segments)
        .with_sync(StatusBarSync::create(label, kind))
        .dom()
}

extern "C" fn on_pane_resize(mut data: RefAny, mut info: CallbackInfo, pane: usize, ratio: f32) -> Update {
    with_state(&mut data, &mut info, |s, _, _| {
        match pane {
            0 => s.nav.navigation_ratio = ratio,
            1 => s.nav.list_ratio = ratio,
            _ => {}
        }
        Update::DoNothing
    })
}

/// The window's shortcuts (the editor's own are in `editor.rs`).
extern "C" fn on_window_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let keyboard = info.get_current_keyboard_state();
    let Some(key) = keyboard.current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let modifiers = info.get_key_modifiers();
    let primary = modifiers.ctrl || modifiers.meta;
    let shift = modifiers.shift;
    with_state(&mut data, &mut info, |s, info, app| match (key, primary, shift) {
        (VirtualKeyCode::N, true, false) => {
            info.prevent_default();
            jobs::new_note(info, app, s);
            Update::RefreshDom
        }
        (VirtualKeyCode::K, true, false) => {
            info.prevent_default();
            s.overlay = if s.overlay == Overlay::Palette {
                Overlay::None
            } else {
                s.palette_query.clear();
                Overlay::Palette
            };
            Update::RefreshDom
        }
        (VirtualKeyCode::K, true, true) => {
            info.prevent_default();
            open_link_sheet(s);
            Update::RefreshDom
        }
        (VirtualKeyCode::S, true, false) => {
            info.prevent_default();
            if let Some(note) = s.open_note_mut() {
                note.mark_dirty();
            }
            let started = jobs::save_all(info, app, s, true);
            crate::refresh_if(started)
        }
        (VirtualKeyCode::P, true, true) => {
            info.prevent_default();
            toggle_pin(info, app, s)
        }
        (VirtualKeyCode::H, true, true) => {
            info.prevent_default();
            show_history(info, app, s);
            Update::RefreshDom
        }
        (VirtualKeyCode::Comma, true, false) => {
            info.prevent_default();
            s.screen = Screen::Settings;
            println!("AZNOTES_SCREEN settings");
            Update::RefreshDom
        }
        (VirtualKeyCode::Escape, _, _) => {
            if s.overlay != Overlay::None {
                s.overlay = Overlay::None;
                Update::RefreshDom
            } else if s.screen != Screen::Notes {
                s.screen = Screen::Notes;
                s.history = None;
                println!("AZNOTES_SCREEN notes");
                Update::RefreshDom
            } else {
                Update::DoNothing
            }
        }
        _ => Update::DoNothing,
    })
}

/// Files dropped on the window: images go into the open note, after the
/// block the caret was last in.
extern "C" fn on_dropped_file(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let files: Vec<PathBuf> = info
        .get_dropped_files()
        .as_ref()
        .iter()
        .map(|f| PathBuf::from(f.as_str()))
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| {
                    matches!(
                        e.to_ascii_lowercase().as_str(),
                        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff"
                    )
                })
        })
        .collect();
    if files.is_empty() {
        return Update::DoNothing;
    }
    with_state(&mut data, &mut info, |s, info, app| {
        let Some(note) = s.open_note() else {
            return Update::DoNothing;
        };
        let job = Job::Import {
            id: note.id.clone(),
            prefix: note.assets_prefix(),
            sources: files,
            at: s.editor.caret_block,
        };
        jobs::spawn(info, app, s, job);
        Update::DoNothing
    })
}

// ==== The navigation pane ====

/// The notebooks in the tree's depth-first order (index 0 is the tree's
/// root, "Notebooks"): what a click on node `i` names.
fn notebook_order(nodes: &[NotebookNode], out: &mut Vec<String>) {
    for node in nodes {
        out.push(node.path.clone());
        notebook_order(&node.children, out);
    }
}

fn notebook_node(node: &NotebookNode, s: &AppState) -> TreeViewNode {
    let selected = s.query.scope == Scope::Notebook(node.path.clone());
    let mut tree = TreeViewNode::create(format!("{} ({})", node.name, node.count))
        .with_icon("folder")
        .with_selected(selected)
        .with_expanded(s.nav.expanded.contains(&node.path));
    for child in &node.children {
        tree = tree.with_child(notebook_node(child, s));
    }
    tree
}

/// The tags in their tree's order (after the root, "Tags").
fn tag_order(s: &AppState) -> Vec<String> {
    s.library.tags().into_iter().map(|(t, _)| t).collect()
}

fn navigation_pane(s: &AppState, app: &RefAny) -> Dom {
    let counts = s.library.counts();
    let library = TreeViewNode::create(format!("All notes ({})", counts.all))
        .with_icon("notes")
        .with_expanded(true)
        .with_selected(s.query.scope == Scope::All)
        .with_child(
            TreeViewNode::create(format!("Pinned ({})", counts.pinned))
                .with_icon("push_pin")
                .with_selected(s.query.scope == Scope::Pinned),
        )
        .with_child(
            TreeViewNode::create(format!("Trash ({})", counts.trash))
                .with_icon("delete")
                .with_selected(s.query.scope == Scope::Trash),
        );
    let tree = s.library.notebook_tree();
    let mut notebooks = TreeViewNode::create("Notebooks")
        .with_icon("menu_book")
        .with_expanded(true);
    for node in &tree {
        notebooks = notebooks.with_child(notebook_node(node, s));
    }
    let mut tags = TreeViewNode::create("Tags").with_icon("sell").with_expanded(true);
    for (tag, count) in s.library.tags() {
        let selected = matches!(&s.query.scope, Scope::Tag(t) if t.eq_ignore_ascii_case(&tag));
        tags = tags.with_child(
            TreeViewNode::create(format!("#{tag} ({count})"))
                .with_icon("tag")
                .with_selected(selected),
        );
    }
    let header = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center;")
        .with_child(
            Button::create("New note")
                .with_icon("note_add")
                .with_button_type(ButtonType::Primary)
                .with_on_click(app.clone(), on_new_note as ButtonOnClickCallbackType)
                .dom()
                .with_id("new-note")
                .with_css("margin-right: 6px;"),
        )
        .with_child(
            Button::create("")
                .with_icon("create_new_folder")
                .with_on_click(app.clone(), on_new_notebook as ButtonOnClickCallbackType)
                .dom()
                .with_id("new-notebook")
                .with_accessibility_name("New notebook"),
        );
    ShellNavigationPane::create()
        .with_header(header)
        .with_group(
            ShellNavigationGroup::create("Library", library)
                .with_count(counts.all)
                .with_open(s.nav.groups_open[0]),
        )
        .with_group(
            ShellNavigationGroup::create("Notebooks", notebooks)
                .with_count(tree.len())
                .with_open(s.nav.groups_open[1]),
        )
        .with_group(ShellNavigationGroup::create("Tags", tags).with_open(s.nav.groups_open[2]))
        .with_label("Notebooks and tags")
        .with_collapsed(s.nav.collapsed)
        .with_on_event(app.clone(), on_nav_event as ShellNavigationPaneOnEventCallbackType)
        .dom()
}

/// Shows `scope`; the open note stays open when the list holds it, else
/// the list's first note opens.
pub fn show_scope(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState, scope: Scope) {
    s.query.scope = scope;
    s.screen = Screen::Notes;
    let rows = s
        .library
        .rows(&s.query, azul_storage::time::now_unix(), AppState::utc_offset());
    let listed: Vec<String> = rows
        .iter()
        .filter_map(|r| match r {
            ListRow::Note(i) => Some(s.library.notes[*i].id.clone()),
            ListRow::Section(_) => None,
        })
        .collect();
    let keep = s.open.as_ref().is_some_and(|id| listed.contains(id));
    if !keep {
        if let Some(first) = listed.first() {
            let first = first.clone();
            jobs::open_note(info, app, s, &first);
        }
    }
}

extern "C" fn on_nav_event(mut data: RefAny, mut info: CallbackInfo, event: ShellNavigationPaneEvent) -> Update {
    with_state(&mut data, &mut info, |s, info, app| {
        match event.kind {
            ShellNavigationPaneEventKind::NodeClicked => {
                let scope = match (event.group, event.index) {
                    (0, 1) => Scope::Pinned,
                    (0, 2) => Scope::Trash,
                    (1, i) if i > 0 => {
                        let mut order = Vec::new();
                        notebook_order(&s.library.notebook_tree(), &mut order);
                        match order.get(i - 1) {
                            Some(path) => Scope::Notebook(path.clone()),
                            None => Scope::All,
                        }
                    }
                    (2, i) if i > 0 => match tag_order(s).get(i - 1) {
                        Some(tag) => Scope::Tag(tag.clone()),
                        None => Scope::All,
                    },
                    _ => Scope::All,
                };
                println!("AZNOTES_SCOPE {}", scope.label());
                show_scope(info, app, s, scope);
            }
            ShellNavigationPaneEventKind::NodeToggled => {
                if event.group == 1 && event.index > 0 {
                    let mut order = Vec::new();
                    notebook_order(&s.library.notebook_tree(), &mut order);
                    if let Some(path) = order.get(event.index - 1).cloned() {
                        if event.expand {
                            s.nav.expanded.insert(path);
                        } else {
                            s.nav.expanded.remove(&path);
                        }
                    }
                }
            }
            ShellNavigationPaneEventKind::GroupToggled => {
                if let Some(open) = s.nav.groups_open.get_mut(event.group) {
                    *open = event.expand;
                }
            }
            ShellNavigationPaneEventKind::CollapseToggled => s.nav.collapsed = !event.expand,
            ShellNavigationPaneEventKind::ModuleSelected => {}
        }
        Update::RefreshDom
    })
}

extern "C" fn on_new_note(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |s, info, app| {
        jobs::new_note(info, app, s);
        Update::RefreshDom
    })
}

extern "C" fn on_new_notebook(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |s, _, _| {
        s.overlay = Overlay::NewNotebook {
            name: String::new(),
            error: String::new(),
        };
        Update::RefreshDom
    })
}
