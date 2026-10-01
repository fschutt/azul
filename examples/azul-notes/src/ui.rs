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

// ==== The note list ====

/// The list's rows as the model orders them now (a callback reads the same
/// rows the layout drew).
fn list_rows(s: &AppState) -> Vec<ListRow> {
    s.library
        .rows(&s.query, azul_storage::time::now_unix(), AppState::utc_offset())
}

/// The second line of a row: the notebook and the tags.
fn row_detail(note: &model::Note) -> String {
    let mut out = note.home_notebook().replace('/', " \u{203a} ");
    for tag in &note.meta.tags {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push('#');
        out.push_str(tag);
    }
    out
}

/// A row's glyph: what kind of note it is at a glance.
fn row_icon(note: &model::Note) -> &'static str {
    if !note.doc.image_srcs().is_empty() {
        "image"
    } else if note.doc.checklist().1 > 0 {
        "checklist"
    } else if note
        .doc
        .blocks
        .iter()
        .any(|b| matches!(b.kind, BlockKind::Code { .. }))
    {
        "code"
    } else {
        "description"
    }
}

fn note_list(s: &AppState, app: &RefAny) -> Dom {
    let now = azul_storage::time::now_unix();
    let offset = AppState::utc_offset();
    let rows = s.library.rows(&s.query, now, offset);
    let any_note = rows.iter().any(|r| matches!(r, ListRow::Note(_)));
    let mut message_rows = Vec::with_capacity(rows.len());
    for row in &rows {
        match row {
            ListRow::Section(title) => message_rows.push(MessageRow::create_group(title.as_str())),
            ListRow::Note(i) => {
                let note = &s.library.notes[*i];
                let date = if s.query.sort == SortKey::Created {
                    note.meta.created
                } else {
                    note.meta.modified
                };
                // The row's id is its note's place in the library, plus one
                // (0 is "no row" in the list's events).
                let id = u64::try_from(*i + 1).unwrap_or(0);
                let mut preview = model::row_preview(note);
                if preview.is_empty() {
                    preview = String::from("No text");
                }
                message_rows.push(
                    MessageRow::create(id, note.display_title(), preview)
                        .with_preview(row_detail(note))
                        .with_date(model::short_date(date, now, offset))
                        .with_icon(row_icon(note))
                        .with_flagged(note.meta.pinned)
                        .with_selected(s.open.as_deref() == Some(note.id.as_str())),
                );
            }
        }
    }
    let cb = on_list_event as MessageListOnEventCallbackType;
    let list = MessageList::create(MessageRowVec::from_vec(message_rows))
        .with_search(s.query.search.as_str())
        .with_search_placeholder("Search notes")
        .with_sort("Arrange by:", s.query.sort.label(), s.query.descending)
        .with_sort_direction_label(s.query.sort.direction_label(s.query.descending))
        .with_mark(MessageListMark::Pin)
        .with_row_height(64)
        .with_on_select(app.clone(), cb)
        .with_on_open(app.clone(), cb)
        .with_on_flag(app.clone(), cb)
        .with_on_delete(app.clone(), cb)
        .with_on_sort(app.clone(), cb)
        .with_on_search(app.clone(), cb)
        .dom();
    let mut column = Dom::create_div()
        .with_id("note-list")
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;");
    if !s.notice.is_empty() {
        column.add_child(
            Dom::create_div()
                .with_id("notice")
                .with_css("padding: 6px 10px; font-size: 12px;")
                .with_child(text_line(&s.notice, "")),
        );
    }
    column.add_child(list);
    if !any_note {
        let (title, detail) = if !s.query.search.trim().is_empty() {
            (
                "Nothing matches".to_string(),
                format!("No note holds \"{}\" in {}.", s.query.search.trim(), s.query.scope.label()),
            )
        } else if s.query.scope == Scope::Trash {
            ("The Trash is empty".to_string(), String::from("Deleted notes wait here."))
        } else if s.library.notes.is_empty() {
            (
                "No notes yet".to_string(),
                String::from("Notes are Markdown files in your AzNotes folder."),
            )
        } else {
            (format!("No notes in {}", s.query.scope.label()), String::new())
        };
        column.add_child(
            ShellEmptyState::create(title)
                .with_icon("note_add")
                .with_detail(detail)
                .with_action_label("New note")
                .with_on_action(app.clone(), on_new_note as ButtonOnClickCallbackType)
                .dom(),
        );
    }
    column
}

/// The note of row `event.id` (the library index plus one).
fn event_note(s: &AppState, event: &MessageListEvent) -> Option<String> {
    let index = usize::try_from(event.id).ok()?.checked_sub(1)?;
    s.library.notes.get(index).map(|n| n.id.clone())
}

extern "C" fn on_list_event(mut data: RefAny, mut info: CallbackInfo, event: MessageListEvent) -> Update {
    with_state(&mut data, &mut info, |s, info, app| match event.kind {
        MessageListEventKind::Select => {
            if let Some(id) = event_note(s, &event) {
                jobs::open_note(info, app, s, &id);
            }
            Update::RefreshDom
        }
        MessageListEventKind::Open => {
            if let Some(id) = event_note(s, &event) {
                jobs::open_note(info, app, s, &id);
                editor::focus_editor(info);
            }
            Update::RefreshDom
        }
        MessageListEventKind::Flag => match event_note(s, &event) {
            Some(id) => {
                if let Some(note) = s.library.get_mut(&id) {
                    note.meta.pinned = !note.meta.pinned;
                    note.mark_dirty();
                }
                jobs::save_note(info, app, s, &id, false);
                Update::RefreshDom
            }
            None => Update::DoNothing,
        },
        MessageListEventKind::Delete => match event_note(s, &event) {
            Some(id) => {
                delete_note(info, app, s, &id);
                Update::RefreshDom
            }
            None => Update::DoNothing,
        },
        MessageListEventKind::Sort => {
            s.query.sort = s.query.sort.next();
            s.query.descending = s.query.sort != SortKey::Title;
            Update::RefreshDom
        }
        MessageListEventKind::SortDirection => {
            s.query.descending = !s.query.descending;
            Update::RefreshDom
        }
        MessageListEventKind::Search => {
            s.query.search = event.text.as_str().to_string();
            Update::RefreshDom
        }
        MessageListEventKind::Scope | MessageListEventKind::Scroll => Update::DoNothing,
    })
}

/// Pins or unpins the open note.
pub fn toggle_pin(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState) -> Update {
    let Some(id) = s.open.clone() else {
        return Update::DoNothing;
    };
    if let Some(note) = s.library.get_mut(&id) {
        note.meta.pinned = !note.meta.pinned;
        note.mark_dirty();
    }
    jobs::save_note(info, app, s, &id, false);
    Update::RefreshDom
}

/// Opens the list's first note other than `id`, or none.
fn open_next(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState, id: &str) {
    let next = list_rows(s).into_iter().find_map(|r| match r {
        ListRow::Note(i) if s.library.notes[i].id != id => Some(s.library.notes[i].id.clone()),
        _ => None,
    });
    match next {
        Some(next) => jobs::open_note(info, app, s, &next),
        None => {
            if let Some(host) = editor::host_node(info, editor::root_dom()) {
                info.reset_editor_content(host, false);
            }
            s.open = None;
        }
    }
}

/// Delete in the list: a note goes to the Trash; in the Trash, a
/// confirmation deletes it for good.
pub fn delete_note(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState, id: &str) {
    let Some(note) = s.library.get(id) else {
        return;
    };
    if note.is_trashed() {
        s.overlay = Overlay::ConfirmDelete { id: id.to_string() };
        return;
    }
    let trash = model::trash_path(note.home_notebook());
    if let Some(note) = s.library.get_mut(id) {
        note.move_to(&trash);
    }
    if s.open.as_deref() == Some(id) {
        open_next(info, app, s, id);
    }
    jobs::save_note(info, app, s, id, false);
}

/// Restores a trashed note into its notebook.
pub fn restore_note(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState, id: &str) {
    let Some(home) = s.library.get(id).map(|n| n.home_notebook().to_string()) else {
        return;
    };
    if let Some(note) = s.library.get_mut(id) {
        note.move_to(&home);
    }
    jobs::save_note(info, app, s, id, false);
}

/// Deletes a note for good: its file, its images, its versions.
pub fn delete_forever(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState, id: &str) {
    let Some(note) = s.library.get(id) else {
        return;
    };
    let mut keys = vec![note.key()];
    if let Some(old) = &note.moved_from {
        keys.push(old.clone());
    }
    let job = Job::Delete {
        id: id.to_string(),
        keys,
        prefixes: vec![note.assets_prefix(), model::history_prefix(id)],
    };
    let saved = !note.saved.is_empty();
    if s.open.as_deref() == Some(id) {
        open_next(info, app, s, id);
    }
    s.library.notes.retain(|n| n.id != id);
    if saved {
        jobs::spawn(info, app, s, job);
    }
}
