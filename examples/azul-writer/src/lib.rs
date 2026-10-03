//! AzWriter: a Word-style document editor on the public azul API.
//!
//! The window is the S1 `DocumentShell` (the app-drawn `Titlebar` under
//! `WindowDecorations::NoTitle`, the ribbon or the backstage, the A4 pages,
//! the status bar) inside a `ShellThemeScope`, behind a `CloseGuard` that
//! asks "Save changes?"; it follows the app theme (flat / flora) and the
//! OS mode. The text is azul's shared rich-text editor (`RichTextEditor` +
//! `RichTextDoc`, the one AzNotes and AzMail's compose window use): one
//! document, ONE undo history (Ctrl/Cmd+Z included), formats from the
//! engine, one editing host per page (`RichTextEditor::page_doms`, the
//! pages from the engine's pagination). Documents are Markdown files in the
//! data tree, `writer/<uuid>.md`, exports in `writer/exports/`, written
//! through azul-storage from a `Thread` (azul-appkit's file jobs); args,
//! settings, About and the shortcuts are azul-appkit's.
//!
//! On stdout, for scripts (`scripts/azwriter_e2e.py`): `AZWRITER_READY`,
//! `AZWRITER_LISTED <n>`, `AZWRITER_OPENED <id>`, `AZWRITER_SAVED <id>`,
//! `AZWRITER_EXPORTED <key>`, `AZWRITER_PAGES <n>`, `AZWRITER_DELETED <id>`.

pub mod app;
mod backstage;
pub mod commands;
pub mod docx;
pub mod ids;
pub mod model;
pub mod pages;
pub mod paginate;
mod ribbon;
pub mod storage;

use azul::{
    app::App,
    callbacks::{CallbackInfo, LayoutCallbackInfo, RefAny, TimerCallbackInfo, TimerCallbackReturn, Update},
    css::EventFilter,
    dom::{Dom, VirtualKeyCode},
    font::FontCacheSnapshot,
    shells::{DocumentShell, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    widgets::{
        AboutDialog, CloseGuard, CloseGuardEvent, CloseGuardEventKind, Modal, ModalState,
        StandardDialogEvent,
    },
    window::WindowEventFilter,
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec},
    files::FileOutcome,
    shortcuts::Shortcut,
    ui as kit,
};

pub use crate::app::AppState;
use crate::{
    app::{BackstagePage, Command, Screen},
    model::DocumentModel,
};

// ==== The app's facts ====

pub const SCREENS: [&str; 4] = ["editor", "open", "new", "settings"];

pub const SPEC: AppSpec = AppSpec {
    name: "AzWriter",
    binary: "AzWriter",
    summary: "documents on A4 pages, kept as Markdown files",
    screens: &SCREENS,
    files_help: "Markdown (.md) or Word (.docx) files to import as new documents",
};

pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzWriter",
    version: env!("CARGO_PKG_VERSION"),
    summary: "A document editor with A4 pages: headings, lists, tables, page breaks, Word import \
              and PDF export. Every document is a Markdown file in your data folder.",
    license: "MIT",
    app_folder: storage::APP_FOLDER,
};

pub const SHORTCUTS: [Shortcut; 14] = [
    Shortcut::new("Document", "Mod+N", "New document"),
    Shortcut::new("Document", "Mod+O", "Open a document"),
    Shortcut::new("Document", "Mod+S", "Save"),
    Shortcut::new("Document", "Mod+P", "Export as PDF"),
    Shortcut::new("Editing", "Mod+Z", "Undo"),
    Shortcut::new("Editing", "Mod+Shift+Z / Mod+Y", "Redo"),
    Shortcut::new("Editing", "Mod+B / Mod+I / Mod+U", "Bold / italic / underline"),
    Shortcut::new("Editing", "Mod+Shift+X", "Strikethrough"),
    Shortcut::new("Editing", "Mod+E", "Inline code"),
    Shortcut::new("Editing", "Mod+0 / 1 / 2 / 3", "Normal text / heading 1 - 3"),
    Shortcut::new("Editing", "Mod+Shift+7 / 8 / 9", "Numbered / bulleted / check list"),
    Shortcut::new("Editing", "Tab / Shift+Tab", "Indent / outdent a list item"),
    Shortcut::new("Window", "Escape", "Leave File, close About"),
    Shortcut::new("Window", "F6", "The next pane"),
];

// ==== Start ====

/// Reads a file named on the command line as a document to import (at
/// start, before the window - not from a callback).
fn import_at_start(path: &std::path::Path) -> Option<azul::widgets::RichTextDoc> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[azwriter] cannot read {}: {e}", path.display());
            return None;
        }
    };
    match commands::import_bytes(&path.display().to_string(), &bytes) {
        Ok(doc) => Some(doc),
        Err(e) => {
            eprintln!("[azwriter] cannot import {}: {e}", path.display());
            None
        }
    }
}

pub fn start() {
    let args = match AppArgs::from_env(&SPEC) {
        Ok(a) => a,
        Err(message) => {
            println!("{message}");
            std::process::exit(if message.contains("USAGE") { 0 } else { 2 });
        }
    };
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &[], args.clone());
    let data_root = {
        let mut k = kit_ref.clone();
        let root = k.downcast_ref::<kit::Kit>().map(|k| k.data_root.clone());
        root.unwrap_or_default()
    };
    let mut st = AppState::new(kit_ref.clone(), data_root, args.sample);
    st.doc = Some(DocumentModel::untitled(model::new_document_id()));
    if let Some(doc) = args.files.first().and_then(|p| import_at_start(p)) {
        st.doc = Some(DocumentModel::from_doc(model::new_document_id(), doc, String::new()));
        st.save_on_start = true;
    }
    match args.screen.as_deref() {
        Some("open") => {
            st.screen = Screen::Backstage;
            st.backstage = BackstagePage::Open;
        }
        Some("new") => {
            st.screen = Screen::Backstage;
            st.backstage = BackstagePage::New;
        }
        Some("settings") => kit::open_settings(&kit_ref, None),
        _ => {}
    }
    let config = kit::app_config(&kit_ref);
    let window = kit::window_options(&kit_ref, layout, (1280.0, 800.0), (720.0, 480.0), on_window_created);
    App::create(RefAny::new(st), config).run(window);
}

// ==== The window ====

fn column(children: Vec<Dom>) -> Dom {
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;");
    for child in children {
        column.add_child(child);
    }
    column
}

/// The About box (open while `about_open`).
fn about_modal(app: &RefAny, st: &AppState) -> Dom {
    let about = AboutDialog::create(AzString::from(ABOUT.name), AzString::from(format!("Version {}", ABOUT.version)))
        .with_icon(AzString::from("description"))
        .with_description(AzString::from(ABOUT.summary))
        .with_copyright(AzString::from("Copyright 2026 the azul contributors"))
        .with_credit(AzString::from("azul"), AzString::from("MIT"))
        .with_credit(AzString::from("docx-parser"), AzString::from("MIT"))
        .with_on_event(app.clone(), on_about)
        .dom()
        .with_id(ids::ABOUT);
    Modal::create(about)
        .with_title(AzString::from("About AzWriter"))
        .with_open(st.about_open)
        .with_on_close(app.clone(), on_modal_close)
        .dom()
}

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode and the theme makes a switch of either rebuild the window.
    let _mode = info.get_mode();
    let _theme = info.get_theme();
    let width = info.get_window_width();
    let app = data.clone();
    // The window's fonts, for the pagination thread.
    if let Some(mut st) = data.downcast_mut::<AppState>() {
        if st.fonts.is_none() {
            st.fonts = Some(FontCacheSnapshot::from_layout_info(&info));
        }
    }
    let Some(guard) = data.downcast_ref::<AppState>() else {
        return Dom::create_body();
    };
    let st = &*guard;
    let title = st.title();
    let content = if kit::settings_open(&st.kit) {
        column(vec![kit::title_row(&title), kit::settings_page(&st.kit, Vec::new())])
    } else {
        match st.screen {
            Screen::Backstage => DocumentShell::create(Dom::create_div())
                .office_shell()
                .with_title_row(kit::title_row(&title))
                .with_backstage(backstage::backstage(&app, st))
                .dom(),
            Screen::Editor => DocumentShell::create(pages::document_area(&app, st, width))
                .office_shell()
                .with_title_row(kit::title_row(&title))
                .with_ribbon(ribbon::ribbon(&app, st))
                .with_status_bar(pages::status_bar(&app, st))
                .dom(),
        }
    };
    let document_name = st.doc.as_ref().map_or_else(|| "AzWriter".to_string(), DocumentModel::title);
    let guarded = CloseGuard::create(content, AzString::from(document_name))
        .with_dirty(st.is_dirty())
        .with_asking(st.asking_close)
        .with_on_event(app.clone(), on_close_guard)
        .dom();
    let root = column(vec![guarded, about_modal(&app, st)]);
    Dom::create_body()
        .with_css("display: flex; flex-direction: column; margin: 0px; padding: 0px; height: 100%;")
        .with_child(ShellThemeScope::create(root).with_accent(ShellThemeAccent::Blue).dom())
        .with_callback(EventFilter::Window(WindowEventFilter::VirtualKeyDown), app, on_key)
}

// ==== Callbacks ====

extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    kit::on_window_created(&st.kit, &mut info);
    commands::list_documents(st, &mut info, &app);
    if st.save_on_start {
        st.save_on_start = false;
        commands::save(st, &mut info, &app, storage::tag::SAVE);
    }
    // The pagination keeps up with the document (one at a time).
    let timer = Timer::create(app.clone(), pagination_tick, info.get_system_time_fn())
        .with_interval(Duration::System(SystemTimeDiff::from_millis(400)));
    info.add_timer(TimerId::unique(), timer);
    println!("AZWRITER_READY");
    Update::DoNothing
}

extern "C" fn pagination_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let app = data.clone();
    if let Some(mut st) = data.downcast_mut::<AppState>() {
        paginate::ensure(&mut st, &mut info.callback_info, &app);
    }
    TimerCallbackReturn::continue_unchanged()
}

/// The sample document of `--sample`: made once, when the data folder has
/// no document and the open one is the untouched blank one.
fn make_sample(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) {
    if !st.sample || st.sample_done || !st.docs.is_empty() || st.is_dirty() {
        return;
    }
    st.sample_done = true;
    let doc = DocumentModel::from_doc(
        model::new_document_id(),
        azul::widgets::RichTextDoc::create_from_markdown(storage::SAMPLE),
        String::new(),
    );
    commands::show_document(st, doc);
    commands::save(st, info, app, storage::tag::SAVE);
}

/// The answers of the file jobs (`storage::tag`).
pub extern "C" fn on_files_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let handle = app.clone();
    let Some(mut guard) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    let mut close = false;
    for outcome in reply.outcomes {
        match outcome {
            FileOutcome::GotAll { files, errors, .. } => {
                st.docs = storage::entries_from(&files);
                st.listed = true;
                if let Some(e) = errors.first() {
                    st.notice = format!("Some documents could not be read: {e}");
                }
                println!("AZWRITER_LISTED {}", st.docs.len());
            }
            FileOutcome::Got { key, result } => match result {
                Ok(Some(bytes)) => {
                    let id = storage::id_of_key(&key).unwrap_or_default().to_string();
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    commands::show_document(st, DocumentModel::from_markdown(id, &text));
                }
                Ok(None) => st.notice = "That document is gone.".to_string(),
                Err(e) => st.notice = format!("The document could not be read: {e}"),
            },
            FileOutcome::Put { key, result } => {
                let save = storage::id_of_key(&key).map(str::to_string);
                match (save, result) {
                    (Some(id), Ok(())) => {
                        if let Some(at) = st.pending_saves.iter().position(|(i, _)| *i == id) {
                            let (_, markdown) = st.pending_saves.remove(at);
                            if let Some(doc) = st.doc.as_mut().filter(|d| d.id == id) {
                                doc.saved = markdown;
                            }
                        }
                        println!("AZWRITER_SAVED {id}");
                        close |= reply.tag == storage::tag::SAVE_AND_CLOSE;
                    }
                    (Some(id), Err(e)) => {
                        st.pending_saves.retain(|(i, _)| *i != id);
                        st.close_after_save = false;
                        st.notice = format!("The document could not be saved: {e}");
                    }
                    (None, Ok(())) => {
                        st.notice = format!(
                            "Exported to {}",
                            azul_appkit::data::local_path(&st.data_root, &key).display()
                        );
                        println!("AZWRITER_EXPORTED {key}");
                    }
                    (None, Err(e)) => st.notice = format!("The export failed: {e}"),
                }
            }
            FileOutcome::Deleted { key, result } => match result {
                Ok(()) => println!(
                    "AZWRITER_DELETED {}",
                    storage::id_of_key(&key).unwrap_or_default()
                ),
                Err(e) => st.notice = format!("The document could not be deleted: {e}"),
            },
        }
    }
    if reply.tag == storage::tag::LIST {
        make_sample(st, &mut info, &handle);
    }
    if reply.tag == storage::tag::SAVE && st.screen == Screen::Backstage && st.backstage == BackstagePage::Open {
        commands::list_documents(st, &mut info, &handle);
    }
    if close && st.close_after_save && !st.is_saving() {
        info.close_window();
    }
    Update::RefreshDom
}

/// The close guard: a close was asked while the document has changes, or
/// the question was answered.
extern "C" fn on_close_guard(mut data: RefAny, mut info: CallbackInfo, event: CloseGuardEvent) -> Update {
    let handle = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    match event.kind {
        CloseGuardEventKind::Ask => {
            commands::sync_doc(st, &mut info);
            st.asking_close = true;
        }
        CloseGuardEventKind::Save => {
            st.asking_close = false;
            st.close_after_save = true;
            commands::save(st, &mut info, &handle, storage::tag::SAVE_AND_CLOSE);
        }
        CloseGuardEventKind::Discard | CloseGuardEventKind::Cancel => st.asking_close = false,
    }
    Update::RefreshDom
}

extern "C" fn on_about(mut data: RefAny, _info: CallbackInfo, _event: StandardDialogEvent) -> Update {
    if let Some(mut st) = data.downcast_mut::<AppState>() {
        st.about_open = false;
    }
    Update::RefreshDom
}

extern "C" fn on_modal_close(mut data: RefAny, _info: CallbackInfo, _state: ModalState) -> Update {
    if let Some(mut st) = data.downcast_mut::<AppState>() {
        st.about_open = false;
    }
    Update::RefreshDom
}

/// The window's keys: the kit's first (settings, F1), then the document's.
/// The text's keys (formats, undo) are the editor's.
extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((kit_ref, screen, about_open)) = data
        .downcast_ref::<AppState>()
        .map(|s| (s.kit.clone(), s.screen, s.about_open))
    else {
        return Update::DoNothing;
    };
    if let Some(update) = kit::handle_key(&kit_ref, &mut info) {
        return update;
    }
    if kit::settings_open(&kit_ref) {
        return Update::DoNothing;
    }
    let Some(key) = info.get_current_keyboard_state().current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let modifiers = info.get_key_modifiers();
    let primary = modifiers.primary_down();
    let cmd = match (key, primary, modifiers.shift) {
        (VirtualKeyCode::S, true, false) => Command::Save,
        (VirtualKeyCode::N, true, false) => Command::NewDocument,
        (VirtualKeyCode::O, true, false) => Command::OpenBackstage(BackstagePage::Open),
        (VirtualKeyCode::P, true, false) => Command::ExportPdf,
        (VirtualKeyCode::Escape, false, _) if about_open => {
            if let Some(mut st) = data.downcast_mut::<AppState>() {
                st.about_open = false;
            }
            info.prevent_default();
            return Update::RefreshDom;
        }
        (VirtualKeyCode::Escape, false, _) if screen == Screen::Backstage => Command::CloseBackstage,
        _ => return Update::DoNothing,
    };
    info.prevent_default();
    commands::run(&mut data, cmd, &mut info)
}

#[cfg(target_os = "android")]
#[ctor::ctor]
fn azul_android_init() {
    start();
}
