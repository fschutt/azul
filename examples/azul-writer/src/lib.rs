//! AzWriter: a Word-style document editor on the public azul API.
//!
//! The window is the S1 `DocumentShell` (`WindowDecorations::NoTitle`: the
//! ribbon, whose tabs are the title bar, or the backstage under the
//! app-drawn `Titlebar`; the A4 pages, the status bar) inside a
//! `ShellThemeScope`, behind a `CloseGuard` that
//! asks "Save changes?"; it follows the app theme (flat / flora) and the
//! OS mode. The text is azul's shared rich-text editor (`RichTextEditor` +
//! `RichTextDoc`, the one AzNotes and AzMail's compose window use): one
//! document, ONE undo history (Ctrl/Cmd+Z included), formats from the
//! engine, one editing host per page (`RichTextEditor::page_doms`, the
//! pages from the engine's pagination). Documents are Markdown files in the
//! data tree, `writer/<uuid>.md`, exports in `writer/exports/`, written
//! through azul-storage from a `Thread` (azul-appkit's file jobs); args,
//! settings, About and the shortcuts are azul-appkit's, and so is the update banner
//! ("A new version of AzWriter is available"; AzInstallManager installs it).
//!
//! On stdout, for scripts (`scripts/azwriter_e2e.py`): `AZWRITER_READY`,
//! `AZWRITER_LISTED <n>`, `AZWRITER_OPENED <id>`, `AZWRITER_SAVED <id>`,
//! `AZWRITER_EXPORTED <key>`, `AZWRITER_PAGES <n>`, `AZWRITER_DELETED <id>`.

pub mod app;
mod backstage;
pub mod commands;
pub mod docx;
pub mod ids;
pub mod l10n;
#[cfg(test)]
mod l10n_tests;
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
        AboutDialog, CloseGuardEvent, CloseGuardEventKind, Modal, ModalState,
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
    // The About page says it by `azwriter-about-summary` (l10n::app_word).
    summary: "A document editor with A4 pages: headings, lists, tables, page breaks, Word import \
              and PDF export. Every document is a Markdown file in your data folder.",
    license: "MIT",
    app_folder: storage::APP_FOLDER,
};

pub const SHORTCUTS: [Shortcut; 14] = [
    Shortcut::new("azwriter-shortcut-document", "Mod+N", "azwriter-new-document"),
    Shortcut::new("azwriter-shortcut-document", "Mod+O", "azwriter-open-document"),
    Shortcut::new("azwriter-shortcut-document", "Mod+S", "azwriter-save"),
    Shortcut::new("azwriter-shortcut-document", "Mod+P", "azwriter-export-pdf"),
    Shortcut::new("azwriter-group-editing", "Mod+Z", "azwriter-undo"),
    Shortcut::new("azwriter-group-editing", "Mod+Shift+Z / Mod+Y", "azwriter-redo"),
    Shortcut::new("azwriter-group-editing", "Mod+B / Mod+I / Mod+U", "azwriter-shortcut-bold"),
    Shortcut::new("azwriter-group-editing", "Mod+Shift+X", "azwriter-strikethrough"),
    Shortcut::new("azwriter-group-editing", "Mod+E", "azwriter-shortcut-code"),
    Shortcut::new("azwriter-group-editing", "Mod+0 / 1 / 2 / 3", "azwriter-shortcut-styles"),
    Shortcut::new("azwriter-group-editing", "Mod+Shift+7 / 8 / 9", "azwriter-shortcut-lists"),
    Shortcut::new("azwriter-group-editing", "Tab / Shift+Tab", "azwriter-shortcut-indent"),
    Shortcut::new("azwriter-group-window", "Escape", "azwriter-shortcut-escape"),
    Shortcut::new("azwriter-group-window", "F6", "azwriter-shortcut-next-pane"),
];

// ==== Start ====

/// Reads a file named on the command line as a document to import (at
/// start, before the window - not from a callback), through the kit's one
/// way to read a file outside the data tree.
fn import_at_start(path: &std::path::Path) -> Option<azul::widgets::RichTextDoc> {
    let read = azul_appkit::files::read_outside(path).map(Some);
    match commands::imported(&path.display().to_string(), read) {
        Ok(doc) => Some(doc),
        Err(e) => {
            eprintln!("[azwriter] {e}");
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
    // AzWriter is always set in flora's blue, whatever the other apps show.
    kit::pin_theme(&kit_ref, azul_appkit::Theme::Flora);
    let data_root = {
        let mut k = kit_ref.clone();
        let root = k.downcast_ref::<kit::Kit>().map(|k| k.data_root.clone());
        root.unwrap_or_default()
    };
    // appkit's words and AzWriter's before any is said; the language chosen (Settings,
    // `--language`) says them from here.
    let mut config = kit::app_config(&kit_ref);
    crate::l10n::register(&mut config);
    // AzInstallManager keeps AzWriter current: the registration, the periodic check, the banner.
    azul_appkit::update::app::start(&kit_ref, &mut config, "azwriter");
    let mut kit_now = kit_ref.clone();
    let language = kit_now.downcast_ref::<kit::Kit>().map(|k| k.language());
    if let Some(language) = language.filter(|l| *l != azul_appkit::args::LanguagePref::System) {
        azul_appkit::l10n::set_locale(language.tag());
    }
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
    let version = azul_appkit::l10n::t_args(
        "azwriter-version",
        &[("version", azul_appkit::l10n::Arg::from(ABOUT.version))],
    );
    let about = AboutDialog::create(AzString::from(ABOUT.name), AzString::from(version))
        .with_icon(AzString::from("description"))
        .with_description(AzString::from(azul_appkit::l10n::app_word(
            ABOUT.name,
            "about-summary",
            ABOUT.summary,
        )))
        .with_copyright(AzString::from("Copyright 2026 the azul contributors"))
        .with_credit(AzString::from("azul"), AzString::from("MIT"))
        .with_credit(AzString::from("docx-parser"), AzString::from("MIT"))
        .with_on_event(app.clone(), on_about)
        .dom()
        .with_id(ids::ABOUT);
    Modal::create(about)
        .with_title(azul_appkit::l10n::label("azwriter-about"))
        .with_open(st.about_open)
        .with_on_close(app.clone(), on_modal_close)
        .dom()
}

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // The layout's language says the words (a switch of it too).
    azul_appkit::l10n::begin_layout(&info);
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
            // No title row: the ribbon's tabs are the title bar.
            Screen::Editor => DocumentShell::create(pages::document_area(&app, st, width))
                .office_shell()
                .with_ribbon(ribbon::ribbon(&app, st))
                .with_status_bar(pages::status_bar(&app, st))
                .dom(),
        }
    };
    let document_name = st.doc.as_ref().map_or_else(|| "AzWriter".to_string(), DocumentModel::title);
    let guarded = kit::close_guard(content, &document_name)
        .with_dirty(st.is_dirty())
        .with_asking(st.asking_close)
        .with_on_event(app.clone(), on_close_guard)
        .dom();
    // A newer AzWriter waiting: the update kit's banner over everything else.
    let mut children: Vec<Dom> = azul_appkit::update::app::banner(&st.kit).into_iter().collect();
    children.push(guarded);
    children.push(about_modal(&app, st));
    let root = column(children);
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
                    st.notice = azul_appkit::l10n::t_args(
                        "azwriter-some-unread",
                        &[("why", azul_appkit::l10n::Arg::from(e.to_string()))],
                    );
                }
                println!("AZWRITER_LISTED {}", st.docs.len());
            }
            FileOutcome::Got { key, result } if reply.tag == storage::tag::IMPORT => {
                commands::finish_import(st, &mut info, &handle, &key, result);
            }
            FileOutcome::Got { key, result } => match result {
                Ok(Some(bytes)) => {
                    let id = storage::id_of_key(&key).unwrap_or_default().to_string();
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    commands::show_document(st, DocumentModel::from_markdown(id, &text));
                }
                Ok(None) => st.notice = azul_appkit::l10n::t("azwriter-doc-gone"),
                Err(e) => {
                    st.notice = azul_appkit::l10n::t_args(
                        "azwriter-doc-unread",
                        &[("why", azul_appkit::l10n::Arg::from(e.to_string()))],
                    );
                }
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
                        st.notice = azul_appkit::l10n::t_args(
                            "azwriter-doc-unsaved",
                            &[("why", azul_appkit::l10n::Arg::from(e.to_string()))],
                        );
                    }
                    (None, Ok(())) => {
                        st.notice = azul_appkit::l10n::t_args(
                            "azwriter-exported",
                            &[(
                                "path",
                                azul_appkit::l10n::Arg::from(
                                    azul_appkit::data::local_path(&st.data_root, &key).display().to_string(),
                                ),
                            )],
                        );
                        println!("AZWRITER_EXPORTED {key}");
                    }
                    (None, Err(e)) => {
                        st.notice = azul_appkit::l10n::t_args(
                            "azwriter-export-failed",
                            &[("why", azul_appkit::l10n::Arg::from(e.to_string()))],
                        );
                    }
                }
            }
            FileOutcome::Deleted { key, result } => match result {
                Ok(()) => println!(
                    "AZWRITER_DELETED {}",
                    storage::id_of_key(&key).unwrap_or_default()
                ),
                Err(e) => {
                    st.notice = azul_appkit::l10n::t_args(
                        "azwriter-doc-undeleted",
                        &[("why", azul_appkit::l10n::Arg::from(e.to_string()))],
                    );
                }
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
        // The guard closes the window itself.
        CloseGuardEventKind::Discard => st.discard_document(),
        CloseGuardEventKind::Cancel => st.asking_close = false,
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
