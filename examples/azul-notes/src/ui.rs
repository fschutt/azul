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
        SummaryListOnEventCallbackType, RefAny, SegmentedOnChangeCallbackType,
        ShellCommandPaletteOnQueryCallbackType, ShellCommandPaletteOnRunCallbackType,
        ShellNavigationPaneOnEventCallbackType, ShellOnPaneResizeCallbackType,
        StandardDialogOnEventCallbackType, TextInputOnFocusLostCallbackType, TextInputOnTextInputCallbackType,
        TextInputOnVirtualKeyDownCallbackType, ToolbarOnEventCallbackType, Update,
    },
    css::{ColorU, DarkLightMode, EventFilter},
    dom::{Dom, VirtualKeyCode},
    option::OptionDarkLightMode,
    shells::{
        PimShell, ShellCommandPalette, ShellEmptyState, ShellNavigationGroup,
        ShellNavigationPane, ShellNavigationPaneEvent, ShellNavigationPaneEventKind,
        ShellPaletteCommand, ShellThemeAccent, ShellThemeScope,
    },
    str::String as AzString,
    vec::{SummaryRowVec, StringVec},
    widgets::{
        AboutDialog, Button, ButtonType, Chip, ChipState, SummaryList, SummaryListEvent,
        SummaryListEventKind, SummaryListMark, SummaryRow, OnTextInputReturn, RichBlockKind,
        RichCheck, RichFormat, RichTextCommand, Segmented, SegmentedState, StatusBar,
        StatusBarSegment, StatusBarSync, StatusBarSyncKind, TextInput, TextInputState,
        TextInputValid, Titlebar, Toolbar, ToolbarEvent, ToolbarItem, TreeViewNode, Modal,
        ModalState, StandardDialogEvent,
    },
    window::WindowEventFilter,
};

use azul_appkit::{
    args::{ModePref, Theme},
    settings::AppSettings,
    ui::{self as kit, AppSection},
};

use crate::{
    ids,
    editor, jobs,
    look::{self, Look, TextSize},
    model::{self, ListRow, NotebookNode, Scope, SortKey},
    store::Job,
    with_state, AppState, HistoryView, Overlay, Screen, Status,
};

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
    // Reading the mode makes a light / dark switch rebuild the window (a
    // theme switch always does).
    let look = look::of(
        look::is_flora(info.get_theme().as_str()),
        matches!(info.get_mode(), DarkLightMode::Dark),
    );
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<AppState>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    let content = if kit::settings_open(&s.kit) {
        settings_screen(s, &app, look)
    } else {
        match s.screen {
            Screen::Notes => notes_screen(s, &app, look),
            Screen::History => history_screen(s, &app, look),
        }
    };
    let area = Dom::create_div()
        .with_id(ids::NOTES_AREA)
        .with_css("position: relative; display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(content)
        .with_child(overlay_dom(s, &app, look))
        .with_child(about_modal(s, &app));
    let column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(title_row(look))
        .with_child(area);
    // No font here: the theme scope sets the hand every text inherits (the
    // system's UI font under flat, flora's Garamond under flora).
    Dom::create_body()
        .with_css("display: flex; flex-direction: column; height: 100%; margin: 0px;")
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
/// lights): the chrome's colour, no line of its own. Under flora the
/// Titlebar's own flora look (the window chrome band, Garamond).
fn title_row(look: &Look) -> Dom {
    if look.flora {
        return Titlebar::create("AzNotes").without_border_bottom().dom();
    }
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
        .with_list_label("Notes")
        .with_navigation_ratio(s.nav.navigation_ratio)
        .with_list_ratio(s.nav.list_ratio)
        .office_shell()
        .with_status_bar(status_bar(s))
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
    let primary = modifiers.primary_down();
    let shift = modifiers.shift;
    // The kit's keys first: Mod+, (settings), F1 (the shortcuts), Escape
    // (leave the settings).
    if let Some(kit_ref) = data.downcast_ref::<AppState>().map(|s| s.kit.clone()) {
        if let Some(update) = kit::handle_key(&kit_ref, &mut info) {
            return update;
        }
        if kit::settings_open(&kit_ref) {
            return Update::DoNothing;
        }
    }
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
            open_link_sheet(s, info);
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
        (VirtualKeyCode::Escape, _, _) => {
            if s.about_open {
                s.about_open = false;
                Update::RefreshDom
            } else if s.overlay != Overlay::None {
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
                .with_id(ids::NEW_NOTE)
                .with_css("margin-right: 6px;"),
        )
        .with_child(
            Button::create("")
                .with_icon("create_new_folder")
                .with_on_click(app.clone(), on_new_notebook as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::NEW_NOTEBOOK)
                .with_accessibility_name("New notebook"),
        )
        .with_child(
            Button::create("")
                .with_icon("settings")
                .with_on_click(app.clone(), on_open_settings as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::OPEN_SETTINGS)
                .with_accessibility_name("Settings")
                .with_css("margin-left: 4px;"),
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
            // Dropping a note on a notebook is not wired yet.
            ShellNavigationPaneEventKind::NodeDropped => {}
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

extern "C" fn on_open_settings(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |s, _, _| {
        kit::open_settings(&s.kit, None);
        s.overlay = Overlay::None;
        println!("AZNOTES_SCREEN settings");
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
    } else if note.doc.checklist_total() > 0 {
        "checklist"
    } else if note
        .doc
        .blocks
        .iter()
        .any(|b| matches!(b.kind, RichBlockKind::Code(_)))
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
            ListRow::Section(title) => message_rows.push(SummaryRow::create_group(title.as_str())),
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
                    SummaryRow::create(id, note.display_title(), preview)
                        .with_preview(row_detail(note))
                        .with_date(model::short_date(date, now, offset))
                        .with_icon(row_icon(note))
                        .with_flagged(note.meta.pinned)
                        .with_selected(s.open.as_deref() == Some(note.id.as_str())),
                );
            }
        }
    }
    let cb = on_list_event as SummaryListOnEventCallbackType;
    let list = SummaryList::create(SummaryRowVec::from_vec(message_rows))
        .with_search(s.query.search.as_str())
        .with_search_placeholder("Search notes")
        .with_sort("Arrange by:", s.query.sort.label(), s.query.descending)
        .with_sort_direction_label(s.query.sort.direction_label(s.query.descending))
        .with_mark(SummaryListMark::Pin)
        .with_row_height(64)
        .with_on_select(app.clone(), cb)
        .with_on_open(app.clone(), cb)
        .with_on_flag(app.clone(), cb)
        .with_on_delete(app.clone(), cb)
        .with_on_sort(app.clone(), cb)
        .with_on_search(app.clone(), cb)
        .dom();
    let mut column = Dom::create_div()
        .with_id(ids::NOTE_LIST)
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;");
    if !s.notice.is_empty() {
        column.add_child(
            Dom::create_div()
                .with_id(ids::NOTICE)
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
fn event_note(s: &AppState, event: &SummaryListEvent) -> Option<String> {
    let index = usize::try_from(event.id).ok()?.checked_sub(1)?;
    s.library.notes.get(index).map(|n| n.id.clone())
}

extern "C" fn on_list_event(mut data: RefAny, mut info: CallbackInfo, event: SummaryListEvent) -> Update {
    with_state(&mut data, &mut info, |s, info, app| match event.kind {
        SummaryListEventKind::Select => {
            if let Some(id) = event_note(s, &event) {
                jobs::open_note(info, app, s, &id);
            }
            Update::RefreshDom
        }
        SummaryListEventKind::Open => {
            if let Some(id) = event_note(s, &event) {
                jobs::open_note(info, app, s, &id);
                editor::focus_editor(info);
            }
            Update::RefreshDom
        }
        SummaryListEventKind::Flag => match event_note(s, &event) {
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
        SummaryListEventKind::Delete => match event_note(s, &event) {
            Some(id) => {
                delete_note(info, app, s, &id);
                Update::RefreshDom
            }
            None => Update::DoNothing,
        },
        SummaryListEventKind::Sort => {
            s.query.sort = s.query.sort.next();
            s.query.descending = s.query.sort != SortKey::Title;
            Update::RefreshDom
        }
        SummaryListEventKind::SortDirection => {
            s.query.descending = !s.query.descending;
            Update::RefreshDom
        }
        SummaryListEventKind::Search => {
            s.query.search = event.text.as_str().to_string();
            Update::RefreshDom
        }
        SummaryListEventKind::Scope | SummaryListEventKind::Scroll => Update::DoNothing,
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

// ==== The editor pane ====

/// A toolbar button: an editor command, or the link sheet.
#[derive(Clone)]
enum Tool {
    Command(RichTextCommand),
    Link,
}

/// The toolbar's buttons: `(id, icon, label, name, tool)`; a label shows as
/// text when there is no icon.
fn tools() -> Vec<(AzString, &'static str, &'static str, &'static str, Tool)> {
    let kind = |kind: RichBlockKind| Tool::Command(RichTextCommand::ToggleKind(kind));
    let format = |format: RichFormat| Tool::Command(RichTextCommand::ToggleFormat(format));
    vec![
        (ids::TOOL_H1, "", "H1", "Heading 1", kind(RichBlockKind::Heading(1))),
        (ids::TOOL_H2, "", "H2", "Heading 2", kind(RichBlockKind::Heading(2))),
        (ids::TOOL_H3, "", "H3", "Heading 3", kind(RichBlockKind::Heading(3))),
        (ids::TOOL_BOLD, "format_bold", "", "Bold", format(RichFormat::Bold)),
        (ids::TOOL_ITALIC, "format_italic", "", "Italic", format(RichFormat::Italic)),
        (ids::TOOL_UNDERLINE, "format_underlined", "", "Underline", format(RichFormat::Underline)),
        (ids::TOOL_STRIKE, "format_strikethrough", "", "Strikethrough", format(RichFormat::Strike)),
        (ids::TOOL_CODE, "code", "", "Inline code", format(RichFormat::Code)),
        (ids::TOOL_BULLETS, "format_list_bulleted", "", "Bulleted list", kind(RichBlockKind::Bullet(0))),
        (ids::TOOL_NUMBERS, "format_list_numbered", "", "Numbered list", kind(RichBlockKind::Numbered(0))),
        (
            ids::TOOL_CHECKLIST,
            "checklist",
            "",
            "Checklist",
            kind(RichBlockKind::Check(RichCheck {
                indent: 0,
                checked: false,
            })),
        ),
        (ids::TOOL_OUTDENT, "format_indent_decrease", "", "Outdent", Tool::Command(RichTextCommand::Outdent)),
        (ids::TOOL_INDENT, "format_indent_increase", "", "Indent", Tool::Command(RichTextCommand::Indent)),
        (ids::TOOL_QUOTE, "format_quote", "", "Quote", Tool::Command(RichTextCommand::ToggleQuote)),
        (
            ids::TOOL_CODEBLOCK,
            "data_object",
            "",
            "Code block",
            kind(RichBlockKind::Code(AzString::from(""))),
        ),
        (ids::TOOL_LINK, "link", "", "Link", Tool::Link),
        (ids::TOOL_RULE, "horizontal_rule", "", "Horizontal rule", Tool::Command(RichTextCommand::InsertRule)),
    ]
}

/// The formatting toolbar, azul's `Toolbar`: the block kind and the formats
/// at the caret show pressed (toggles), the other tools are buttons; each
/// tool's id is its `ids::TOOL_*` (what its event carries).
fn toolbar(s: &AppState, app: &RefAny, look: &Look) -> Dom {
    let mut items = Vec::new();
    for (id, icon, label, name, tool) in tools() {
        let divider = [ids::TOOL_H3, ids::TOOL_CODE, ids::TOOL_INDENT, ids::TOOL_CODEBLOCK].contains(&id);
        let pressed = match &tool {
            Tool::Command(RichTextCommand::ToggleKind(kind)) => Some(s.editor.is_current_kind(kind.clone())),
            Tool::Command(RichTextCommand::ToggleFormat(format)) => {
                Some(s.editor.is_current_format(format.clone()))
            }
            Tool::Command(RichTextCommand::ToggleQuote) => Some(s.editor.is_current_quoted()),
            _ => None,
        };
        // A tool without an icon shows its label (H1 - H3) and says its name as a
        // tooltip; an icon-only tool is named by its label.
        let shown = if icon.is_empty() { label } else { name };
        let item = match pressed {
            Some(on) => ToolbarItem::create_toggle(id, shown, icon, on),
            None => ToolbarItem::create_button(id, shown, icon),
        };
        items.push(if icon.is_empty() { item.with_tooltip(name) } else { item });
        if divider {
            items.push(ToolbarItem::create_separator());
        }
    }
    Dom::create_div()
        .with_id(ids::FORMAT_TOOLBAR)
        .with_css(format!(
            "padding: 4px 24px; border-bottom: 1px solid {}; flex-shrink: 0;",
            look.line
        ))
        .with_child(
            Toolbar::create("Formatting")
                .with_items(items)
                .with_on_event(app.clone(), on_tool_event as ToolbarOnEventCallbackType)
                .dom(),
        )
}

/// A formatting tool was used (a button activated, a toggle switched): its
/// command runs on the editor - a toggle's state is the editor's at the
/// caret on the rebuild - and the caret goes back to the text (unless the
/// link sheet opened).
extern "C" fn on_tool_event(mut data: RefAny, mut info: CallbackInfo, event: ToolbarEvent) -> Update {
    let Some(tool) = tools()
        .into_iter()
        .find(|t| t.0 == event.id)
        .map(|t| t.4)
    else {
        return Update::DoNothing;
    };
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let update = match tool {
        Tool::Command(command) => editor::run(s, &mut info, command),
        Tool::Link => {
            open_link_sheet(s, &mut info);
            Update::RefreshDom
        }
    };
    if !matches!(s.overlay, Overlay::Link { .. }) {
        editor::focus_editor(&mut info);
    }
    update
}

/// The payload of a tag chip.
struct TagRef {
    app: RefAny,
    tag: String,
}

/// The header's buttons: `(id, icon, name, action)`.
fn header_button(app: &RefAny, id: AzString, icon: &str, name: &str, primary: bool, action: ButtonOnClickCallbackType) -> Dom {
    let mut button = Button::create("").with_icon(icon).with_on_click(app.clone(), action);
    if primary {
        button = button.with_button_type(ButtonType::Primary);
    }
    button
        .dom()
        .with_id(id)
        .with_accessibility_name(name)
        .with_css("margin-left: 4px;")
}

/// The editor pane: the note's header (where it is, when it was edited, the
/// note's buttons), its title, its tags, the toolbar and the text.
fn reading_pane(s: &AppState, app: &RefAny, look: &Look) -> Dom {
    let Some(note) = s.open_note() else {
        return ShellEmptyState::create("No note open")
            .with_icon("description")
            .with_detail("Choose a note in the list, or start a new one.")
            .with_action_label("New note")
            .with_on_action(app.clone(), on_new_note as ButtonOnClickCallbackType)
            .dom();
    };
    let offset = AppState::utc_offset();
    let mut pane = Dom::create_div().with_id(ids::EDITOR_PANE).with_css(format!(
        "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; background: {}; color: {};",
        look.paper, look.text
    ));

    if note.is_trashed() {
        pane.add_child(
            Dom::create_div()
                .with_id(ids::TRASH_BAR)
                .with_css(format!(
                    "display: flex; flex-direction: row; align-items: center; padding: 6px 24px; \
                     background: {}; flex-shrink: 0;",
                    look.code_bg
                ))
                .with_child(text_line("This note is in the Trash.", "flex-grow: 1; font-size: 13px;"))
                .with_child(
                    Button::create("Restore")
                        .with_on_click(app.clone(), on_restore as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::RESTORE_NOTE),
                )
                .with_child(
                    Button::create("Delete forever")
                        .with_button_type(ButtonType::Danger)
                        .with_on_click(app.clone(), on_delete_forever as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::DELETE_FOREVER)
                        .with_css("margin-left: 6px;"),
                ),
        );
    }

    // Where the note is and when it was edited, the note's buttons.
    let place = note.home_notebook().replace('/', " \u{203a} ");
    let meta = format!("Edited {} \u{b7} {}", model::long_date(note.meta.modified, offset), place);
    pane.add_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center; padding: 10px 24px 0px 24px; flex-shrink: 0;")
            .with_child(text_line(&meta, &format!("flex-grow: 1; font-size: 12px; color: {};", look.muted)).with_id(ids::NOTE_META))
            .with_child(header_button(
                app,
                ids::PIN_NOTE,
                "push_pin",
                if note.meta.pinned { "Unpin" } else { "Pin" },
                note.meta.pinned,
                on_pin as ButtonOnClickCallbackType,
            ))
            .with_child(header_button(app, ids::NOTE_HISTORY, "history", "Version history", false, on_history as ButtonOnClickCallbackType))
            .with_child(header_button(app, ids::EXPORT_PDF, "picture_as_pdf", "Export as PDF", false, on_export_pdf as ButtonOnClickCallbackType))
            .with_child(header_button(
                app,
                ids::EXPORT_MARKDOWN,
                "file_download",
                "Export as Markdown",
                false,
                on_export_markdown as ButtonOnClickCallbackType,
            ))
            .with_child(header_button(app, ids::TRASH_NOTE, "delete", "Move to Trash", false, on_trash as ButtonOnClickCallbackType)),
    );

    // The title.
    pane.add_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; padding: 6px 24px 0px 24px; flex-shrink: 0;")
            .with_child(
                TextInput::create()
                    .with_text(note.meta.title.as_str())
                    .with_placeholder("Title")
                    .with_accessibility_name("Title")
                    .with_on_text_input(app.clone(), on_title_input as TextInputOnTextInputCallbackType)
                    .with_on_virtual_key_down(app.clone(), on_title_key as TextInputOnVirtualKeyDownCallbackType)
                    .dom()
                    .with_id(ids::NOTE_TITLE)
                    // The title reads as a title: a heading's size and weight
                    // on the field, which its value line inherits (as an
                    // <input>'s font does).
                    .with_css("flex-grow: 1; font-size: 22px; font-weight: bold;"),
            ),
    );

    // The tags: a chip each, then the field that adds one.
    let mut tags = Dom::create_div()
        .with_id(ids::NOTE_TAGS)
        .with_css("display: flex; flex-direction: row; flex-wrap: wrap; align-items: center; padding: 6px 24px; flex-shrink: 0;");
    for tag in &note.meta.tags {
        tags.add_child(
            Chip::create(format!("#{tag}"))
                .with_removable(true)
                .with_on_remove(
                    RefAny::new(TagRef {
                        app: app.clone(),
                        tag: tag.clone(),
                    }),
                    on_tag_remove as ChipOnRemoveCallbackType,
                )
                .dom()
                .with_css("margin-right: 4px;"),
        );
    }
    tags.add_child(
        TextInput::create()
            .with_text(s.tag_draft.as_str())
            .with_placeholder("Add tag")
            .with_accessibility_name("Add tag")
            .with_on_text_input(app.clone(), on_tag_input as TextInputOnTextInputCallbackType)
            .with_on_virtual_key_down(app.clone(), on_tag_key as TextInputOnVirtualKeyDownCallbackType)
            .with_on_focus_lost(app.clone(), on_tag_blur as TextInputOnFocusLostCallbackType)
            .dom()
            .with_id(ids::TAG_INPUT)
            .with_css("width: 140px;"),
    );
    pane.add_child(tags);

    pane.add_child(toolbar(s, app, look));

    // The text, scrolling.
    pane.add_child(
        Dom::create_div()
            .with_id(ids::NOTE_SCROLL)
            .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; overflow-y: auto; padding: 8px 32px 0px 32px;")
            .with_child(editor::editor_dom(s, app, note, look.flora)),
    );
    pane
}

fn text_return(update: Update) -> OnTextInputReturn {
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_title_input(mut data: RefAny, mut info: CallbackInfo, field: TextInputState) -> OnTextInputReturn {
    let title = field.get_text().as_str().to_string();
    with_state(&mut data, &mut info, |s, _, _| {
        if let Some(note) = s.open_note_mut() {
            note.meta.title = title;
        }
        s.edited();
        Update::DoNothing
    });
    text_return(Update::DoNothing)
}

/// Enter in the title goes on in the text.
extern "C" fn on_title_key(_data: RefAny, mut info: CallbackInfo, _field: TextInputState) -> OnTextInputReturn {
    let key = info.get_current_keyboard_state().current_virtual_keycode.into_option();
    if matches!(key, Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter)) {
        editor::focus_editor(&mut info);
    }
    text_return(Update::DoNothing)
}

/// Adds the tag being typed to the open note and clears the field (the
/// field's typing is acknowledged, so the empty field the app renders
/// wins over it). Returns whether a tag was added.
fn commit_tag(s: &mut AppState, info: &mut CallbackInfo) -> bool {
    let draft = s.tag_draft.trim().trim_matches(',').to_string();
    s.tag_draft.clear();
    // The editor's typing first: the acknowledgment covers every editor.
    let _ = editor::sync(s, info);
    info.mark_text_revision_synced(info.get_document_text_revision());
    if draft.is_empty() {
        return false;
    }
    let added = s.open_note_mut().is_some_and(|n| n.add_tag(&draft));
    if added {
        s.edited();
    }
    added
}

extern "C" fn on_tag_input(mut data: RefAny, mut info: CallbackInfo, field: TextInputState) -> OnTextInputReturn {
    let text = field.get_text().as_str().to_string();
    let update = with_state(&mut data, &mut info, |s, info, _| {
        // A comma ends a tag, as Enter does.
        if text.ends_with(',') {
            s.tag_draft = text.trim_end_matches(',').to_string();
            commit_tag(s, info);
            return Update::RefreshDom;
        }
        s.tag_draft = text;
        Update::DoNothing
    });
    text_return(update)
}

extern "C" fn on_tag_key(mut data: RefAny, mut info: CallbackInfo, _field: TextInputState) -> OnTextInputReturn {
    let key = info.get_current_keyboard_state().current_virtual_keycode.into_option();
    if !matches!(key, Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter)) {
        return text_return(Update::DoNothing);
    }
    let update = with_state(&mut data, &mut info, |s, info, _| {
        commit_tag(s, info);
        Update::RefreshDom
    });
    text_return(update)
}

extern "C" fn on_tag_blur(mut data: RefAny, mut info: CallbackInfo, _field: TextInputState) -> Update {
    with_state(&mut data, &mut info, |s, info, _| {
        if s.tag_draft.trim().is_empty() {
            return Update::DoNothing;
        }
        commit_tag(s, info);
        Update::RefreshDom
    })
}

extern "C" fn on_tag_remove(mut data: RefAny, mut info: CallbackInfo, _chip: ChipState) -> Update {
    let (mut app, tag) = match data.downcast_ref::<TagRef>() {
        Some(t) => (t.app.clone(), t.tag.clone()),
        None => return Update::DoNothing,
    };
    let Some(mut guard) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let _ = editor::sync(s, &mut info);
    let removed = s.open_note_mut().is_some_and(|n| n.remove_tag(&tag));
    if removed {
        s.edited();
    }
    crate::refresh_if(removed)
}

extern "C" fn on_pin(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |s, info, app| toggle_pin(info, app, s))
}

extern "C" fn on_trash(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |s, info, app| {
        let Some(id) = s.open.clone() else {
            return Update::DoNothing;
        };
        delete_note(info, app, s, &id);
        Update::RefreshDom
    })
}

extern "C" fn on_restore(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |s, info, app| {
        let Some(id) = s.open.clone() else {
            return Update::DoNothing;
        };
        restore_note(info, app, s, &id);
        Update::RefreshDom
    })
}

extern "C" fn on_delete_forever(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |s, _, _| {
        if let Some(id) = s.open.clone() {
            s.overlay = Overlay::ConfirmDelete { id };
        }
        Update::RefreshDom
    })
}

extern "C" fn on_history(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |s, info, app| {
        show_history(info, app, s);
        Update::RefreshDom
    })
}

/// A file name for an export of `title`.
fn export_name(title: &str, extension: &str) -> String {
    let base: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') { c } else { '_' })
        .collect();
    let base = base.trim();
    format!("{}.{extension}", if base.is_empty() { "note" } else { base })
}

extern "C" fn on_export_pdf(mut data: RefAny, mut info: CallbackInfo) -> Update {
    export_pdf(&mut data, &mut info)
}

/// The open note as a PDF (A4 at 96 dpi), through azul's paged pipeline;
/// the state is let go before the render.
fn export_pdf(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    // The paper is white in every theme (a print); the hand is the theme's:
    // flora's Garamond in flora's ink, flat's sans.
    let flora = look::is_flora(info.get_theme().as_str());
    let (name, dom) = {
        let Some(mut guard) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let _ = editor::sync(s, info);
        let Some(note) = s.open_note() else {
            return Update::DoNothing;
        };
        let ink = if flora { "#262521" } else { look::LIGHT.text };
        let page = Dom::create_body()
            .with_css(format!(
                "margin: 0px; padding: 72px; background: white; color: {ink}; font-family: {};",
                look::paper_font(flora)
            ))
            .with_child(editor::print_dom(
                s,
                note,
                &note.doc,
                note.display_title(),
                look::TextSize::Medium.px_for(flora) - 1.0,
                flora,
            ));
        (export_name(note.display_title(), "pdf"), page)
    };
    let bytes = azul::pdf::Pdf::create()
        .from_dom_in_callback(*info, dom, 794.0, 1123.0)
        .as_ref()
        .to_vec();
    if bytes.is_empty() {
        eprintln!("[aznotes] the PDF export produced no bytes");
        return Update::DoNothing;
    }
    let len = bytes.len();
    if azul::dialog::FileDialog::save_bytes(name.as_str(), "application/pdf", bytes) {
        println!("AZNOTES_EXPORTED pdf {len}");
    }
    Update::DoNothing
}

extern "C" fn on_export_markdown(mut data: RefAny, mut info: CallbackInfo) -> Update {
    export_markdown(&mut data, &mut info)
}

/// The open note's file as it is on disk (front matter and Markdown).
fn export_markdown(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    with_state(data, info, |s, info, _| {
        let _ = editor::sync(s, info);
        let Some(note) = s.open_note() else {
            return Update::DoNothing;
        };
        let bytes = note.to_file().into_bytes();
        let len = bytes.len();
        if azul::dialog::FileDialog::save_bytes(export_name(note.display_title(), "md").as_str(), "text/markdown", bytes) {
            println!("AZNOTES_EXPORTED md {len}");
        }
        Update::DoNothing
    })
}

// ==== Sheets over the window ====

/// A sheet: a panel centred over a backdrop, `id` on the panel.
fn sheet(look: &Look, id: AzString, title: &str, body: Dom, buttons: Dom) -> Dom {
    Dom::create_div()
        .with_id(ids::SHEET_BACKDROP)
        .with_css(format!(
            "position: absolute; left: 0px; top: 0px; right: 0px; bottom: 0px; background: {}; \
             display: flex; flex-direction: column; align-items: center; justify-content: center;",
            look.backdrop
        ))
        .with_child(
            Dom::create_div()
                .with_id(id)
                .with_accessibility_name(title)
                .with_css(format!(
                    "width: 440px; padding: 18px 20px; background: {}; color: {}; border-radius: {}; \
                     border: 1px solid {}; display: flex; flex-direction: column;",
                    look.sheet, look.text, look.sheet_radius, look.line
                ))
                .with_child(text_line(title, "font-size: 16px; font-weight: bold; margin-bottom: 12px;"))
                .with_child(body)
                .with_child(
                    buttons.with_css(
                        "display: flex; flex-direction: row; justify-content: flex-end; margin-top: 14px;",
                    ),
                ),
        )
}

/// A sheet's button.
fn sheet_button(app: &RefAny, id: AzString, label: &str, kind: ButtonType, action: ButtonOnClickCallbackType) -> Dom {
    Button::create(label)
        .with_button_type(kind)
        .with_on_click(app.clone(), action)
        .dom()
        .with_id(id)
        .with_css("margin-left: 6px;")
}

/// What is over the window, if anything.
fn overlay_dom(s: &AppState, app: &RefAny, look: &Look) -> Dom {
    match &s.overlay {
        Overlay::None => Dom::create_div(),
        Overlay::Palette => palette_dom(s, app),
        Overlay::NewNotebook { name, error } => {
            let mut body = Dom::create_div()
                .with_css("display: flex; flex-direction: column;")
                .with_child(text_line(
                    "A name, or a path for a notebook inside another (Work/Ideas).",
                    &format!("font-size: 12px; color: {}; margin-bottom: 8px;", look.muted),
                ))
                .with_child(
                    TextInput::create()
                        .with_text(name.as_str())
                        .with_placeholder("Notebook name")
                        .with_accessibility_name("Notebook name")
                        .with_on_text_input(app.clone(), on_sheet_text as TextInputOnTextInputCallbackType)
                        .with_on_virtual_key_down(app.clone(), on_sheet_key as TextInputOnVirtualKeyDownCallbackType)
                        .dom()
                        .with_id(ids::SHEET_FIELD),
                );
            if !error.is_empty() {
                body.add_child(text_line(error, &format!("font-size: 12px; color: {}; margin-top: 6px;", look.error)));
            }
            let buttons = Dom::create_div()
                .with_child(sheet_button(app, ids::SHEET_CANCEL, "Cancel", ButtonType::Default, on_sheet_cancel))
                .with_child(sheet_button(app, ids::SHEET_OK, "Create", ButtonType::Primary, on_sheet_ok));
            sheet(look, ids::SHEET_NEW_NOTEBOOK, "New notebook", body, buttons)
        }
        Overlay::Link { url, spans, .. } => {
            let hint = if spans.is_empty() {
                "Nothing is selected: the link is added as its own text."
            } else {
                "The selected text links to this address."
            };
            let body = Dom::create_div()
                .with_css("display: flex; flex-direction: column;")
                .with_child(text_line(hint, &format!("font-size: 12px; color: {}; margin-bottom: 8px;", look.muted)))
                .with_child(
                    TextInput::create_url()
                        .with_text(url.as_str())
                        .with_placeholder("https://")
                        .with_accessibility_name("Link address")
                        .with_on_text_input(app.clone(), on_sheet_text as TextInputOnTextInputCallbackType)
                        .with_on_virtual_key_down(app.clone(), on_sheet_key as TextInputOnVirtualKeyDownCallbackType)
                        .dom()
                        .with_id(ids::SHEET_FIELD),
                );
            let mut buttons = Dom::create_div();
            if !url.trim().is_empty() {
                buttons.add_child(sheet_button(app, ids::SHEET_OPEN, "Open", ButtonType::Link, on_link_open));
            }
            let buttons = buttons
                .with_child(sheet_button(app, ids::SHEET_REMOVE, "Remove link", ButtonType::Default, on_link_remove))
                .with_child(sheet_button(app, ids::SHEET_CANCEL, "Cancel", ButtonType::Default, on_sheet_cancel))
                .with_child(sheet_button(app, ids::SHEET_OK, "Link", ButtonType::Primary, on_sheet_ok));
            sheet(look, ids::SHEET_LINK, "Link", body, buttons)
        }
        Overlay::ConfirmDelete { id } => {
            let title = s.library.get(id).map_or(model::UNTITLED, |n| n.display_title());
            let body = text_line(
                &format!("\"{title}\" is deleted for good: its file, its images and its versions."),
                "font-size: 13px;",
            );
            let buttons = Dom::create_div()
                .with_child(sheet_button(app, ids::SHEET_CANCEL, "Cancel", ButtonType::Default, on_sheet_cancel))
                .with_child(sheet_button(app, ids::SHEET_OK, "Delete forever", ButtonType::Danger, on_sheet_ok));
            sheet(look, ids::SHEET_DELETE, "Delete forever?", body, buttons)
        }
    }
}

/// Opens the link sheet for the editor's selection (taken now: the sheet's
/// field takes the focus); its address field starts with the link the
/// selection (or the caret) is on.
pub fn open_link_sheet(s: &mut AppState, info: &mut CallbackInfo) {
    let _ = editor::sync(s, info);
    let spans = editor::selection(s, info);
    let (block, at) = spans
        .first()
        .map_or((s.editor.caret_block, s.editor.caret_byte), |(b, start, _)| (*b, *start));
    let url = s
        .editor
        .doc
        .blocks
        .get(block)
        .and_then(|blk| {
            let mut offset = 0usize;
            blk.runs.iter().find_map(|r| {
                let len = r.text.as_str().len();
                let hit = offset <= at && at < offset + len.max(1);
                offset += len;
                match &r.link {
                    azul::option::OptionString::Some(link) if hit => Some(link.as_str().to_string()),
                    _ => None,
                }
            })
        })
        .unwrap_or_default();
    s.overlay = Overlay::Link { url, spans };
}

extern "C" fn on_sheet_text(mut data: RefAny, mut info: CallbackInfo, field: TextInputState) -> OnTextInputReturn {
    let text = field.get_text().as_str().to_string();
    with_state(&mut data, &mut info, |s, _, _| {
        match &mut s.overlay {
            Overlay::NewNotebook { name, error } => {
                *name = text;
                error.clear();
            }
            Overlay::Link { url, .. } => *url = text,
            _ => {}
        }
        Update::DoNothing
    });
    text_return(Update::DoNothing)
}

extern "C" fn on_sheet_key(mut data: RefAny, mut info: CallbackInfo, _field: TextInputState) -> OnTextInputReturn {
    let key = info.get_current_keyboard_state().current_virtual_keycode.into_option();
    let update = match key {
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) => {
            with_state(&mut data, &mut info, |s, info, app| sheet_ok(info, app, s))
        }
        Some(VirtualKeyCode::Escape) => with_state(&mut data, &mut info, |s, _, _| {
            s.overlay = Overlay::None;
            Update::RefreshDom
        }),
        _ => Update::DoNothing,
    };
    text_return(update)
}

extern "C" fn on_sheet_cancel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |s, info, _| {
        s.overlay = Overlay::None;
        editor::focus_editor(info);
        Update::RefreshDom
    })
}

extern "C" fn on_sheet_ok(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |s, info, app| sheet_ok(info, app, s))
}

/// The link sheet's Open: the address with the system's handler.
extern "C" fn on_link_open(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |s, _, _| {
        if let Overlay::Link { url, .. } = &s.overlay {
            let url = url.trim().to_string();
            if editor::open_url(&url) {
                println!("AZNOTES_OPENED_LINK {url}");
            }
        }
        Update::DoNothing
    })
}

extern "C" fn on_link_remove(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |s, info, _| {
        if let Overlay::Link { url, .. } = &mut s.overlay {
            url.clear();
        }
        apply_link(s, info, None);
        s.overlay = Overlay::None;
        editor::focus_editor(info);
        Update::RefreshDom
    })
}

/// Links the sheet's selection to `url` (`None`: unlinks it); with nothing
/// selected, the address goes in at the caret as its own linked text.
fn apply_link(s: &mut AppState, info: &mut CallbackInfo, url: Option<String>) {
    let Overlay::Link { spans, .. } = s.overlay.clone() else {
        return;
    };
    let _ = editor::link(s, info, &spans, url);
}

/// The sheet's main button (or Enter in its field).
fn sheet_ok(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState) -> Update {
    match s.overlay.clone() {
        Overlay::NewNotebook { name, .. } => match model::clean_notebook_path(&name) {
            Ok(path) => {
                s.library.notebooks.insert(path.clone());
                let mut so_far = String::new();
                for segment in path.split('/') {
                    if !so_far.is_empty() {
                        so_far.push('/');
                    }
                    so_far.push_str(segment);
                    s.nav.expanded.insert(so_far.clone());
                }
                s.overlay = Overlay::None;
                let job = Job::PutText {
                    key: model::marker_key(&path),
                    text: String::new(),
                };
                jobs::spawn(info, app, s, job);
                println!("AZNOTES_NOTEBOOK {path}");
                show_scope(info, app, s, Scope::Notebook(path));
                Update::RefreshDom
            }
            Err(e) => {
                s.overlay = Overlay::NewNotebook {
                    name,
                    error: e.to_string(),
                };
                Update::RefreshDom
            }
        },
        Overlay::Link { url, .. } => {
            let url = url.trim().to_string();
            apply_link(s, info, if url.is_empty() { None } else { Some(url) });
            s.overlay = Overlay::None;
            editor::focus_editor(info);
            Update::RefreshDom
        }
        Overlay::ConfirmDelete { id } => {
            s.overlay = Overlay::None;
            delete_forever(info, app, s, &id);
            Update::RefreshDom
        }
        Overlay::None | Overlay::Palette => Update::DoNothing,
    }
}

// ==== The command palette ====

/// What a palette command does.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Action {
    NewNote,
    NewNotebook,
    Open(String),
    MoveTo(String),
    Pin,
    History,
    ExportPdf,
    ExportMarkdown,
    Trash,
    Show(Scope),
    /// The settings page at this category (`None`: the first).
    Settings(Option<&'static str>),
    About,
    Theme(&'static str),
    Mode(&'static str),
}

/// The palette's table: `(action, label, category, icon, shortcut)`, built
/// from the state alone (a run names a row of the same table).
fn palette_actions(s: &AppState) -> Vec<(Action, String, &'static str, &'static str, &'static str)> {
    let mut out = vec![
        (Action::NewNote, "New note".to_string(), "Notes", "note_add", "Ctrl+N"),
        (Action::NewNotebook, "New notebook".to_string(), "Notes", "create_new_folder", ""),
    ];
    if let Some(note) = s.open_note() {
        out.push((
            Action::Pin,
            if note.meta.pinned { "Unpin note" } else { "Pin note" }.to_string(),
            "Note",
            "push_pin",
            "Ctrl+Shift+P",
        ));
        out.push((Action::History, "Version history".to_string(), "Note", "history", "Ctrl+Shift+H"));
        out.push((Action::ExportPdf, "Export as PDF".to_string(), "Note", "picture_as_pdf", ""));
        out.push((Action::ExportMarkdown, "Export as Markdown".to_string(), "Note", "file_download", ""));
        out.push((Action::Trash, "Move to Trash".to_string(), "Note", "delete", ""));
        let here = note.home_notebook().to_string();
        for path in s.library.notebook_paths() {
            if path != here {
                out.push((Action::MoveTo(path.clone()), format!("Move to {path}"), "Move", "drive_file_move", ""));
            }
        }
    }
    out.push((Action::Show(Scope::All), "Show all notes".to_string(), "Go", "notes", ""));
    out.push((Action::Show(Scope::Pinned), "Show pinned notes".to_string(), "Go", "push_pin", ""));
    out.push((Action::Show(Scope::Trash), "Show the Trash".to_string(), "Go", "delete", ""));
    for path in s.library.notebook_paths() {
        out.push((Action::Show(Scope::Notebook(path.clone())), format!("Show {path}"), "Go", "folder", ""));
    }
    for (tag, _) in s.library.tags() {
        out.push((Action::Show(Scope::Tag(tag.clone())), format!("Show #{tag}"), "Go", "tag", ""));
    }
    out.push((Action::Settings(None), "Settings".to_string(), "App", "settings", "Ctrl+,"));
    out.push((Action::Settings(Some("Shortcuts")), "Keyboard shortcuts".to_string(), "App", "keyboard", "F1"));
    out.push((Action::About, "About AzNotes".to_string(), "App", "info", ""));
    out.push((Action::Theme("flat"), "Theme: Flat".to_string(), "App", "palette", ""));
    out.push((Action::Theme("flora"), "Theme: Flora".to_string(), "App", "palette", ""));
    out.push((Action::Mode("light"), "Mode: Light".to_string(), "App", "light_mode", ""));
    out.push((Action::Mode("dark"), "Mode: Dark".to_string(), "App", "dark_mode", ""));
    out.push((Action::Mode("system"), "Mode: Follow the system".to_string(), "App", "contrast", ""));
    // Jump to a note: the most recently edited first.
    let mut notes: Vec<&model::Note> = s.library.notes.iter().filter(|n| !n.is_trashed()).collect();
    notes.sort_by(|a, b| b.meta.modified.cmp(&a.meta.modified));
    for note in notes.into_iter().take(500) {
        out.push((Action::Open(note.id.clone()), note.display_title().to_string(), "Open", "description", ""));
    }
    out
}

fn palette_dom(s: &AppState, app: &RefAny) -> Dom {
    let commands: Vec<ShellPaletteCommand> = palette_actions(s)
        .into_iter()
        .map(|(_, label, category, icon, shortcut)| {
            let mut c = ShellPaletteCommand::create(label).with_category(category).with_icon(icon);
            if !shortcut.is_empty() {
                c = c.with_shortcut(shortcut);
            }
            c
        })
        .collect();
    ShellCommandPalette::create()
        .with_commands(commands)
        .with_query(s.palette_query.as_str())
        .with_placeholder("Open a note, move it, run a command")
        .with_open(true)
        .with_on_query(app.clone(), on_palette_query as ShellCommandPaletteOnQueryCallbackType)
        .with_on_run(app.clone(), on_palette_run as ShellCommandPaletteOnRunCallbackType)
        .with_on_close(app.clone(), on_palette_close as ButtonOnClickCallbackType)
        .dom()
}

extern "C" fn on_palette_query(mut data: RefAny, mut info: CallbackInfo, query: AzString) -> Update {
    let query = query.as_str().to_string();
    with_state(&mut data, &mut info, |s, _, _| {
        s.palette_query = query;
        Update::RefreshDom
    })
}

extern "C" fn on_palette_close(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |s, _, _| {
        s.overlay = Overlay::None;
        Update::RefreshDom
    })
}

/// Applies the app theme / mode and keeps it in the settings.
fn set_look(info: &mut CallbackInfo, _app: &RefAny, s: &mut AppState, theme: Option<&str>, mode: Option<&str>) {
    {
        let mut kit_ref = s.kit.clone();
        let Some(mut k) = kit_ref.downcast_mut::<kit::Kit>() else {
            return;
        };
        if let Some(theme) = theme.and_then(Theme::parse) {
            k.settings.theme = theme;
            k.args.theme = None;
            info.set_theme(theme.name());
        }
        if let Some(mode) = mode.and_then(ModePref::parse) {
            k.settings.mode = mode;
            k.args.mode = None;
            info.set_mode(match mode {
                ModePref::Light => OptionDarkLightMode::Some(DarkLightMode::Light),
                ModePref::Dark => OptionDarkLightMode::Some(DarkLightMode::Dark),
                ModePref::System => OptionDarkLightMode::None,
            });
        }
    }
    kit::save_settings(&s.kit, info);
}

/// Writes AzNotes' values into the kit's settings file (on a Thread).
fn save_settings(info: &mut CallbackInfo, _app: &RefAny, s: &mut AppState) {
    for (key, value) in s.settings.values() {
        kit::set_value(&s.kit, info, key, &value);
    }
}

extern "C" fn on_palette_run(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    // An export renders with the state let go: it runs after the palette
    // closed.
    let mut export: Option<Action> = None;
    let update = with_state(&mut data, &mut info, |s, info, app| {
        let Some((action, label, ..)) = palette_actions(s).into_iter().nth(index) else {
            return Update::DoNothing;
        };
        println!("AZNOTES_RUN {label}");
        s.overlay = Overlay::None;
        s.palette_query.clear();
        match action {
            Action::NewNote => jobs::new_note(info, app, s),
            Action::NewNotebook => {
                s.overlay = Overlay::NewNotebook {
                    name: String::new(),
                    error: String::new(),
                };
            }
            Action::Open(id) => {
                s.screen = Screen::Notes;
                if !s.library.get(&id).is_some_and(|n| model::in_scope(n, &s.query.scope)) {
                    s.query.scope = Scope::All;
                }
                jobs::open_note(info, app, s, &id);
                jobs::focus_editor_soon(info);
            }
            Action::MoveTo(path) => {
                if let Some(id) = s.open.clone() {
                    if let Some(note) = s.library.get_mut(&id) {
                        note.move_to(&path);
                    }
                    jobs::save_note(info, app, s, &id, false);
                }
            }
            Action::Pin => {
                toggle_pin(info, app, s);
            }
            Action::History => show_history(info, app, s),
            Action::ExportPdf | Action::ExportMarkdown => export = Some(action),
            Action::Trash => {
                if let Some(id) = s.open.clone() {
                    delete_note(info, app, s, &id);
                }
            }
            Action::Show(scope) => show_scope(info, app, s, scope),
            Action::Settings(category) => {
                kit::open_settings(&s.kit, category);
                println!("AZNOTES_SCREEN settings");
            }
            Action::About => s.about_open = true,
            Action::Theme(theme) => set_look(info, app, s, Some(theme), None),
            Action::Mode(mode) => set_look(info, app, s, None, Some(mode)),
        }
        Update::RefreshDom
    });
    match export {
        Some(Action::ExportPdf) => {
            export_pdf(&mut data, &mut info);
        }
        Some(Action::ExportMarkdown) => {
            export_markdown(&mut data, &mut info);
        }
        _ => {}
    }
    update
}

// ==== Settings ====

/// Which setting a segmented control sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Setting {
    TextSize,
    Autosave,
    Versions,
}

/// The payload of a setting's control.
struct SettingRef {
    app: RefAny,
    setting: Setting,
}

const AUTOSAVE_MS: [u64; 3] = [500, 2000, 5000];
const VERSION_MINUTES: [u64; 4] = [0, 5, 15, 60];

/// A segmented control for `setting`.
fn choice(app: &RefAny, setting: Setting, labels: &[&str], selected: usize, id: AzString) -> Dom {
    Segmented::create(strs(labels))
        .with_selected_index(selected)
        .with_on_change(
            RefAny::new(SettingRef {
                app: app.clone(),
                setting,
            }),
            on_setting as SegmentedOnChangeCallbackType,
        )
        .dom()
        .with_id(id)
}

/// A paragraph of a settings section.
fn info_text(text: &str, look: &Look) -> Dom {
    text_line(text, &format!("font-size: 13px; color: {}; margin-bottom: 6px;", look.text))
}

/// AzNotes' own sections of the settings page (the kit adds Appearance,
/// Data, Shortcuts and About): Editor (category 0), Storage (category 1).
fn app_sections(s: &AppState, app: &RefAny, look: &Look) -> Vec<AppSection> {
    let section = |category: usize, title: &str, content: Dom| AppSection {
        category,
        title: title.to_string(),
        content,
    };
    let folder = s.root.join(model::APP_FOLDER);
    vec![
        section(
            0,
            "Text size",
            choice(
                app,
                Setting::TextSize,
                &["Small", "Medium", "Large"],
                TextSize::ALL.iter().position(|t| *t == s.settings.text_size).unwrap_or(1),
                ids::SETTING_TEXT_SIZE,
            ),
        ),
        section(
            0,
            "Save after a pause of",
            choice(
                app,
                Setting::Autosave,
                &["0.5 s", "2 s", "5 s"],
                AUTOSAVE_MS.iter().position(|m| *m == s.settings.autosave_ms).unwrap_or(0),
                ids::SETTING_AUTOSAVE,
            ),
        ),
        section(
            0,
            "Keep a version",
            choice(
                app,
                Setting::Versions,
                &["Every save", "Every 5 minutes", "Every 15 minutes", "Every hour"],
                VERSION_MINUTES
                    .iter()
                    .position(|m| *m == s.settings.version_minutes)
                    .unwrap_or(1),
                ids::SETTING_VERSIONS,
            ),
        ),
        section(
            0,
            "Markdown shortcuts",
            info_text(
                "Type # , ## , ### , - , 1. , [ ] , > or ``` at the start of a line to make it a \
                 heading, a list item, a check item, a quote or a code block.",
                look,
            ),
        ),
        section(
            1,
            "Files",
            Dom::create_div()
                .with_css("display: flex; flex-direction: column;")
                .with_child(info_text(&format!("Notes folder: {}", folder.display()), look).with_id(ids::SETTING_FOLDER))
                .with_child(info_text(
                    "Every note is a Markdown file, notes/<notebook>/<id>.md, with its title, tags, \
                     pin and dates in a front matter; its images sit in notes/<notebook>/<id>/assets/, \
                     its versions in notes/.history/<id>/, deleted notes in notes/.trash/. Edit them \
                     with any editor: AzNotes reads them again when its window gets the focus.",
                    look,
                ))
                .with_child(
                    Button::create("Read the folder again")
                        .with_icon("refresh")
                        .with_on_click(app.clone(), on_reload as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::SETTING_RELOAD),
                ),
        ),
    ]
}

/// The settings page: azul-appkit's (Outlook's Options dialog), AzNotes' categories first.
fn settings_screen(s: &AppState, app: &RefAny, look: &Look) -> Dom {
    Dom::create_div()
        .with_id(ids::SETTINGS)
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(kit::settings_page_with_reload(
            &s.kit,
            app_sections(s, app, look),
            app,
            reload_settings,
        ))
}

/// Cancel on the settings page put the settings back: AzNotes' copy of its values follows.
fn reload_settings(app: &mut RefAny, _info: &mut CallbackInfo, settings: &AppSettings) {
    let read = crate::Settings::from_values(|key| settings.get(key).map(str::to_string));
    if let Some(mut s) = app.downcast_mut::<AppState>() {
        s.settings = read;
    };
}

extern "C" fn on_setting(mut data: RefAny, mut info: CallbackInfo, control: SegmentedState) -> Update {
    let (mut app, setting) = match data.downcast_ref::<SettingRef>() {
        Some(r) => (r.app.clone(), r.setting),
        None => return Update::DoNothing,
    };
    let handle = app.clone();
    let Some(mut guard) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let i = control.selected_index;
    match setting {
        Setting::TextSize => s.settings.text_size = TextSize::ALL.get(i).copied().unwrap_or_default(),
        Setting::Autosave => s.settings.autosave_ms = AUTOSAVE_MS.get(i).copied().unwrap_or(500),
        Setting::Versions => s.settings.version_minutes = VERSION_MINUTES.get(i).copied().unwrap_or(5),
    }
    save_settings(&mut info, &handle, s);
    Update::RefreshDom
}

extern "C" fn on_reload(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |s, info, app| {
        let known = s
            .library
            .notes
            .iter()
            .filter(|n| !n.saved.is_empty())
            .map(|n| (n.key(), n.file_modified))
            .collect();
        jobs::spawn(info, app, s, Job::Rescan { known });
        Update::DoNothing
    })
}

/// Back from the history to the notes.
extern "C" fn on_back_to_notes(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |s, _, _| {
        s.screen = Screen::Notes;
        s.history = None;
        println!("AZNOTES_SCREEN notes");
        Update::RefreshDom
    })
}

// ==== About ====

/// The About box: azul's standard AboutDialog in a Modal (open while
/// `about_open`).
fn about_modal(s: &AppState, app: &RefAny) -> Dom {
    let counts = s.library.counts();
    let about = AboutDialog::create("AzNotes", format!("Version {}", crate::ABOUT.version))
        .with_icon("sticky_note_2")
        .with_description(format!(
            "{} {} notes in {} notebooks, {} pinned, {} in the Trash.",
            crate::ABOUT.summary,
            counts.all,
            s.library.notebook_paths().len(),
            counts.pinned,
            counts.trash
        ))
        .with_copyright("Copyright 2026 the azul contributors")
        .with_credit("azul", "MIT")
        .with_on_event(app.clone(), on_about as StandardDialogOnEventCallbackType)
        .dom()
        .with_id(ids::ABOUT);
    Modal::create(about)
        .with_title("About AzNotes")
        .with_open(s.about_open)
        .with_on_close(app.clone(), on_about_close)
        .dom()
}

extern "C" fn on_about(mut data: RefAny, mut info: CallbackInfo, _event: StandardDialogEvent) -> Update {
    with_state(&mut data, &mut info, |s, _, _| {
        s.about_open = false;
        Update::RefreshDom
    })
}

extern "C" fn on_about_close(mut data: RefAny, mut info: CallbackInfo, _state: ModalState) -> Update {
    with_state(&mut data, &mut info, |s, _, _| {
        s.about_open = false;
        Update::RefreshDom
    })
}

// ==== Version history ====

/// Shows the open note's versions (the newest first).
pub fn show_history(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState) {
    let Some(id) = s.open.clone() else {
        return;
    };
    // The note as it is now becomes a version too, once saved.
    if s.open_note().is_some_and(|n| n.dirty) {
        jobs::save_note(info, app, s, &id, true);
    }
    s.history = Some(HistoryView {
        id: id.clone(),
        loading: true,
        ..HistoryView::default()
    });
    s.screen = Screen::History;
    s.overlay = Overlay::None;
    println!("AZNOTES_SCREEN history");
    jobs::spawn(info, app, s, Job::History { id });
}

/// The payload of a version row.
struct VersionRef {
    app: RefAny,
    index: usize,
}

fn history_screen(s: &AppState, app: &RefAny, look: &Look) -> Dom {
    let (Some(view), Some(note)) = (s.history.as_ref(), s.open_note()) else {
        return ShellEmptyState::create("No note open")
            .with_icon("history")
            .with_action_label("Back to notes")
            .with_on_action(app.clone(), on_back_to_notes as ButtonOnClickCallbackType)
            .dom();
    };
    let offset = AppState::utc_offset();
    let header = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 6px 12px; flex-shrink: 0;")
        .with_child(
            Button::create("Back to the note")
                .with_icon("arrow_back")
                .with_on_click(app.clone(), on_back_to_notes as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::HISTORY_BACK),
        )
        .with_child(text_line(
            &format!("History: {}", note.display_title()),
            "flex-grow: 1; font-size: 15px; font-weight: bold; margin-left: 12px;",
        ))
        .with_child(
            Button::create("Restore this version")
                .with_icon("restore")
                .with_button_type(ButtonType::Primary)
                .with_on_click(app.clone(), on_restore_version as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::HISTORY_RESTORE),
        );

    // The versions, newest first.
    let mut list = Dom::create_div().with_id(ids::HISTORY_VERSIONS).with_css(format!(
        "display: flex; flex-direction: column; width: 240px; flex-shrink: 0; overflow-y: auto; \
         border-right: 1px solid {}; padding: 6px;",
        look.line
    ));
    if view.loading {
        list.add_child(text_line("Loading versions...", "font-size: 13px; padding: 6px;"));
    } else if view.versions.is_empty() {
        list.add_child(text_line(
            "No versions yet: AzNotes keeps one per save, at most every few minutes while you type.",
            &format!("font-size: 13px; padding: 6px; color: {};", look.muted),
        ));
    }
    for (index, (_, time)) in view.versions.iter().enumerate() {
        let selected = view.selected == Some(index);
        let mut row = Button::create(model::long_date(*time, offset)).with_on_click(
            RefAny::new(VersionRef {
                app: app.clone(),
                index,
            }),
            on_version_click as ButtonOnClickCallbackType,
        );
        if selected {
            row = row.with_button_type(ButtonType::Primary);
        }
        list.add_child(
            row.dom()
                .with_id(format!("{}{index}", ids::VERSION_PREFIX))
                .with_css("margin-bottom: 4px;"),
        );
    }

    // The selected version, then what changed from it to now.
    let mut detail = Dom::create_div().with_id(ids::HISTORY_DETAIL).with_css(format!(
        "display: flex; flex-direction: column; flex-grow: 1; min-width: 0px; overflow-y: auto; \
         padding: 12px 24px; background: {}; color: {};",
        look.paper, look.text
    ));
    if !view.error.is_empty() {
        detail.add_child(text_line(&view.error, &format!("color: {}; font-size: 13px;", look.error)));
    }
    match &view.text {
        None => detail.add_child(text_line(
            if view.selected.is_some() {
                "Loading the version..."
            } else {
                "Choose a version on the left."
            },
            &format!("font-size: 13px; color: {};", look.muted),
        )),
        Some(text) => {
            let (meta, doc) = crate::markdown::parse_note(text, 0);
            let title = if meta.title.trim().is_empty() {
                model::UNTITLED
            } else {
                meta.title.as_str()
            };
            detail.add_child(
                editor::print_dom(s, note, &doc, title, s.settings.text_size.px_for(look.flora), look.flora)
                    .with_id(ids::HISTORY_VERSION),
            );
            let then = doc.to_markdown();
            let now = note.doc.to_markdown();
            if let Some(diff) = model::line_diff(then.as_str(), now.as_str()) {
                let mut changes = Dom::create_div().with_id(ids::HISTORY_CHANGES).with_css(format!(
                    "display: flex; flex-direction: column; margin-top: 18px; padding-top: 8px; \
                     border-top: 1px solid {}; font-family: monospace; font-size: 12px;",
                    look.line
                ));
                // The heading over the (monospace) diff, in the theme's hand.
                changes.add_child(text_line(
                    "Changes from this version to now",
                    &format!(
                        "font-family: {}; font-weight: bold; margin-bottom: 6px;",
                        look::paper_font(look.flora)
                    ),
                ));
                let changed = diff.iter().filter(|(c, _)| *c != model::Change::Kept).count();
                if changed == 0 {
                    changes.add_child(text_line("No changes.", ""));
                }
                for (change, line) in diff {
                    let (mark, colour) = match change {
                        model::Change::Kept => continue,
                        model::Change::Removed => ("- ", look.error),
                        model::Change::Added => ("+ ", look.accent),
                    };
                    changes.add_child(
                        Dom::create_p_with_text(format!("{mark}{line}"))
                            .with_css(format!("margin: 0px; white-space: pre-wrap; color: {colour};")),
                    );
                }
                detail.add_child(changes);
            }
        }
    }
    Dom::create_div()
        .with_id(ids::HISTORY)
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(header)
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; flex-grow: 1; min-height: 0px;")
                .with_child(list)
                .with_child(detail),
        )
}

extern "C" fn on_version_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let (mut app, index) = match data.downcast_ref::<VersionRef>() {
        Some(r) => (r.app.clone(), r.index),
        None => return Update::DoNothing,
    };
    let handle = app.clone();
    let Some(mut guard) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(view) = s.history.as_mut() else {
        return Update::DoNothing;
    };
    let Some((key, _)) = view.versions.get(index).cloned() else {
        return Update::DoNothing;
    };
    view.selected = Some(index);
    view.text = None;
    view.error.clear();
    let id = view.id.clone();
    jobs::spawn(&mut info, &handle, s, Job::Version { id, key });
    Update::RefreshDom
}

/// Restores the selected version: the note as it is now is kept as a
/// version first, then the version's title, tags and text become the
/// note's (saved, with the editor's content replaced).
extern "C" fn on_restore_version(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |s, info, app| {
        let Some(text) = s.history.as_ref().and_then(|h| h.text.clone()) else {
            return Update::DoNothing;
        };
        let Some(id) = s.open.clone() else {
            return Update::DoNothing;
        };
        let now = azul_storage::time::now_unix();
        if let Some(current) = s.library.get(&id).filter(|n| !n.saved.is_empty()) {
            let job = Job::PutText {
                key: model::history_key(&id, now),
                text: current.saved.clone(),
            };
            jobs::spawn(info, app, s, job);
        }
        let (meta, doc) = crate::markdown::parse_note(&text, now);
        if let Some(host) = editor::host_node(info, editor::root_dom()) {
            info.reset_editor_content(host, false);
        }
        s.editor = editor::state_for(&doc);
        if let Some(note) = s.library.get_mut(&id) {
            note.meta.title = meta.title;
            note.meta.tags = meta.tags;
            note.meta.pinned = meta.pinned;
            note.doc = doc;
            note.touch(now + 1);
            note.refresh();
        }
        s.screen = Screen::Notes;
        s.history = None;
        println!("AZNOTES_RESTORED {id}");
        jobs::save_note(info, app, s, &id, false);
        Update::RefreshDom
    })
}
