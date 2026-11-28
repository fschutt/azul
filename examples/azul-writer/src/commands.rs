//! What the commands do. Every button carries a [`CommandRef`]
//! ([`on_command`]); keys, menus and the sidebar call [`run`].
//!
//! Durable writes (saves, exports, deletes) are azul-appkit file jobs on an
//! azul `Thread` against the data tree; their answers arrive in
//! `crate::on_files_done`.

use azul::{
    callbacks::{CallbackInfo, RefAny, Update},
    dialog::{FileDialog, FileOpenResult},
    dom::Dom,
    file::FileTypeList,
    option::{OptionFileTypeList, OptionString},
    pdf::Pdf,
    str::String as AzString,
};
use azul_appkit::{docs, flow, ui as kit, FileJob};

use crate::{
    app::{AppState, BackstagePage, Command, CommandRef, Screen, ZOOM_MAX, ZOOM_MIN},
    model::{self, DocumentModel},
    paginate, storage,
};

/// A button's click: the command it carries.
pub extern "C" fn on_command(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, cmd)) = data
        .downcast_ref::<CommandRef>()
        .map(|c| (c.app.clone(), c.cmd.clone()))
    else {
        return Update::DoNothing;
    };
    run(&mut app, cmd, &mut info)
}

/// Folds what was typed and not reported yet into the shown document. (The other open documents
/// were folded when they were left.)
pub fn sync_doc(st: &mut AppState, info: &mut CallbackInfo) {
    if let Some(doc) = st.doc_mut() {
        if doc.editor.sync(*info) {
            doc.touch();
        }
    }
}

/// Runs `jobs` on a Thread; the answer comes to `on_files_done` as `tag`.
pub fn spawn(st: &AppState, info: &mut CallbackInfo, app: &RefAny, jobs: Vec<FileJob>, tag: u64) {
    kit::spawn_file_jobs(info, &st.data_root, jobs, app.clone(), tag, crate::on_files_done);
}

/// Reads the list of documents (the backstage's Open page).
pub fn list_documents(st: &AppState, info: &mut CallbackInfo, app: &RefAny) {
    spawn(st, info, app, vec![storage::list_job()], storage::tag::LIST);
}

/// Saves the shown document (the Markdown it is now) under its key; `tag` says what follows the
/// save.
pub fn save(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, tag: u64) {
    sync_doc(st, info);
    let index = st.active;
    save_at(st, info, app, index, tag);
}

/// Saves open document `index` (the Markdown it is now) under its key.
fn save_at(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, index: usize, tag: u64) {
    let Some(doc) = st.open.get(index) else {
        return;
    };
    let markdown = doc.markdown();
    let job = storage::save_job(&doc.id, &markdown);
    st.pending_saves.push((doc.id.clone(), markdown));
    spawn(st, info, app, vec![job], tag);
}

/// Writes every open document that has changes and is not being written already; `tag` says what
/// follows each write (the window closes after the last when it is `SAVE_AND_CLOSE`).
pub fn save_all(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, tag: u64) {
    sync_doc(st, info);
    let dirty: Vec<usize> = (0..st.open.len())
        .filter(|&i| st.open[i].is_dirty() && !st.is_saving_doc(&st.open[i].id))
        .collect();
    for index in dirty {
        save_at(st, info, app, index, tag);
    }
}

/// The app left the foreground (or is about to end): every document with changes is written. A
/// phone ends a background process without another word, so this is the last chance to keep an
/// edit (`lib.rs`: `on_lifecycle`).
pub fn save_on_pause(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) {
    save_all(st, info, app, storage::tag::SAVE);
}

/// The shown document saved first when it has changes (switching documents
/// never loses an edit).
fn save_if_dirty(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) {
    sync_doc(st, info);
    let index = st.active;
    if st.is_dirty() && !st.doc().is_some_and(|d| st.is_saving_doc(&d.id)) {
        save_at(st, info, app, index, storage::tag::SAVE);
    }
}

/// An untouched new document: never saved, nothing typed. Opening another document replaces it
/// rather than leaving an empty tab behind.
#[must_use]
pub fn is_pristine(doc: &DocumentModel) -> bool {
    doc.saved.is_empty() && doc.editor.doc.is_blank()
}

/// `doc` becomes a new open document right after the shown one and takes the screen. A document
/// with this id that is open already is replaced by it (it was read again from its file). `false`
/// when [`docs::MAX_OPEN_DOCS`] are open: nothing opens, the notice says so.
pub fn show_document(st: &mut AppState, doc: DocumentModel) -> bool {
    // the untouched blank document of a new window makes room for the document that replaces it
    if let Some(at) = st.open.iter().position(|d| d.id == doc.id) {
        st.open[at] = doc;
        st.active = at;
        st.screen = Screen::Editor;
        return true;
    }
    if st.open.len() == 1 && st.doc().is_some_and(is_pristine) {
        st.open.clear();
        st.active = 0;
    }
    if !st.can_open_more() {
        st.notice = azul_appkit::l10n::t_args(
            "kit-doc-too-many",
            &[("count", azul_appkit::l10n::Arg::from(docs::MAX_OPEN_DOCS))],
        );
        return false;
    }
    println!("AZWRITER_OPENED {}", doc.id);
    let at = docs::insert_at(st.open.len(), st.active);
    st.open.insert(at, doc);
    st.active = at;
    st.sheet_doc = None;
    st.screen = Screen::Editor;
    true
}

/// Shows open document `index`; the one it takes the screen from is saved when it has changes.
fn switch_to(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, index: usize) -> Update {
    if index >= st.open.len() {
        return Update::DoNothing;
    }
    st.sheet_doc = None;
    st.screen = Screen::Editor;
    if index == st.active {
        return Update::RefreshDom;
    }
    save_if_dirty(st, info, app);
    st.active = index;
    Update::RefreshDom
}

/// The document as printed: on white paper, the A4 margins, the text laid
/// out exactly as the paginator measures it.
#[must_use]
pub fn print_dom(doc: &DocumentModel) -> Dom {
    Dom::create_body()
        .with_css(format!(
            "margin: 0px; padding: {}px; background: #ffffff; color: #1a1a1a;",
            paginate::MARGIN
        ))
        .with_child(paginate::measure_dom(doc.doc()))
}

/// The PDF of the shown document (`None` without one, or when the renderer
/// made nothing).
fn pdf_bytes(doc: &DocumentModel, info: &CallbackInfo) -> Option<Vec<u8>> {
    let bytes = Pdf::create()
        .from_dom_in_callback(*info, print_dom(doc), paginate::A4_W, paginate::A4_H)
        .as_ref()
        .to_vec();
    (!bytes.is_empty()).then_some(bytes)
}

/// The import dialog's filter: Markdown and Word files.
fn import_filter() -> OptionFileTypeList {
    OptionFileTypeList::Some(FileTypeList {
        document_types: vec![AzString::from("*.md"), AzString::from("*.docx")].into(),
        document_descriptor: azul_appkit::l10n::label("azwriter-import-filter"),
    })
}

/// A Markdown or Word file's bytes as a document (`.docx` by name, else
/// UTF-8 Markdown).
pub fn import_bytes(name: &str, bytes: &[u8]) -> Result<azul::widgets::RichTextDoc, String> {
    if name.to_ascii_lowercase().ends_with(".docx") {
        crate::docx::from_docx_bytes(bytes)
    } else {
        Ok(azul::widgets::RichTextDoc::create_from_markdown(
            String::from_utf8_lossy(bytes).into_owned(),
        ))
    }
}

/// What the read of a file to import answered (`name`: the file's name,
/// Word by its `.docx`): the document, or the sentence the user reads.
pub fn imported(name: &str, result: Result<Option<Vec<u8>>, String>) -> Result<azul::widgets::RichTextDoc, String> {
    match result {
        Ok(Some(bytes)) => {
            import_bytes(name, &bytes).map_err(|e| {
                azul_appkit::l10n::t_args(
                    "azwriter-import-failed",
                    &[("name", azul_appkit::l10n::Arg::from(name)), ("why", azul_appkit::l10n::Arg::from(e))],
                )
            })
        }
        Ok(None) => Err(azul_appkit::l10n::t_args(
            "azwriter-import-gone",
            &[("name", azul_appkit::l10n::Arg::from(name))],
        )),
        Err(e) => Err(azul_appkit::l10n::t_args(
            "azwriter-import-unread",
            &[("name", azul_appkit::l10n::Arg::from(name)), ("why", azul_appkit::l10n::Arg::from(e))],
        )),
    }
}

/// The read of a file to import answered (`crate::on_files_done`, tag
/// `IMPORT`): the file becomes a new document of the data tree, saved at
/// once (the shown one saved first when it has changes).
pub fn finish_import(
    st: &mut AppState,
    info: &mut CallbackInfo,
    app: &RefAny,
    name: &str,
    result: Result<Option<Vec<u8>>, String>,
) {
    match imported(name, result) {
        Ok(doc) => {
            save_if_dirty(st, info, app);
            if show_document(st, DocumentModel::from_doc(model::new_document_id(), doc, String::new())) {
                save(st, info, app, storage::tag::SAVE);
                st.notice = azul_appkit::l10n::t_args("azwriter-imported", &[("name", azul_appkit::l10n::Arg::from(name))]);
                println!("AZWRITER_IMPORTED {name}");
            }
        }
        Err(e) => st.notice = e,
    }
}

/// The import dialog answered: the file becomes a new document of the data
/// tree (saved at once).
extern "C" fn on_import_picked(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing;
    };
    let path = std::path::PathBuf::from(path.as_string().as_str());
    let handle = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    // A file the user picked outside the data tree: read on a Thread
    // through a drive at its folder (no manifest there); the answer comes
    // to on_files_done as IMPORT, then the file lives in the tree like any
    // document.
    st.notice = if kit::spawn_outside_read(&mut info, &path, handle, storage::tag::IMPORT, crate::on_files_done) {
        azul_appkit::l10n::t_args(
            "azwriter-importing",
            &[("path", azul_appkit::l10n::Arg::from(path.display().to_string()))],
        )
    } else {
        azul_appkit::l10n::t_args(
            "azwriter-not-importable",
            &[("path", azul_appkit::l10n::Arg::from(path.display().to_string()))],
        )
    };
    Update::RefreshDom
}

/// How many words, characters and pages the shown document has, as the sentence the user reads.
#[must_use]
pub fn word_count_notice(doc: &DocumentModel) -> String {
    let characters: usize = doc
        .doc()
        .blocks
        .as_slice()
        .iter()
        .map(|b| b.get_text().as_str().chars().count())
        .sum();
    let pages = doc.pages.starts_for(doc.doc().block_count()).len();
    azul_appkit::l10n::t_args(
        "azwriter-word-count",
        &[
            ("words", azul_appkit::l10n::Arg::from(doc.word_count())),
            ("characters", azul_appkit::l10n::Arg::from(characters)),
            ("pages", azul_appkit::l10n::Arg::from(pages)),
        ],
    )
}

/// Runs `cmd` on the app.
pub fn run(app: &mut RefAny, cmd: Command, info: &mut CallbackInfo) -> Update {
    let handle = app.clone();
    let Some(mut guard) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    match cmd {
        Command::Rich(rich) => rich_commands(st, info, &handle, vec![rich]),
        Command::Quick(quick) => {
            let Some(doc) = st.doc() else {
                return Update::DoNothing;
            };
            let commands = flow::quick_commands(quick, &doc.editor);
            rich_commands(st, info, &handle, commands)
        }
        Command::RibbonTab(index) => {
            st.ribbon_tab = index;
            Update::RefreshDom
        }
        Command::OpenBackstage(page) => {
            sync_doc(st, info);
            st.screen = Screen::Backstage;
            st.backstage = page;
            if page == BackstagePage::Open {
                list_documents(st, info, &handle);
            }
            if page == BackstagePage::Close {
                return close_document(st, info, &handle);
            }
            Update::RefreshDom
        }
        Command::CloseBackstage => {
            st.screen = Screen::Editor;
            Update::RefreshDom
        }
        Command::NewDocument => {
            // a blank document nobody typed in is the new document already
            if st.doc().is_some_and(is_pristine) {
                st.screen = Screen::Editor;
                return Update::RefreshDom;
            }
            save_if_dirty(st, info, &handle);
            show_document(st, DocumentModel::untitled(model::new_document_id()));
            Update::RefreshDom
        }
        Command::Switch(index) => switch_to(st, info, &handle, index),
        Command::Close(index) => close_at(st, info, &handle, index),
        Command::Collab(index, on) => {
            let Some(doc) = st.open.get_mut(index) else {
                return Update::DoNothing;
            };
            doc.collab = on;
            st.notice = if on {
                azul_appkit::l10n::t(docs::COLLAB_NOTE_KEY)
            } else {
                String::new()
            };
            Update::RefreshDom
        }
        Command::OpenDocument(index) => {
            let Some(id) = st.docs.get(index).map(|d| d.id.clone()) else {
                return Update::DoNothing;
            };
            // open already: it takes the screen
            if let Some(at) = st.index_of(&id) {
                return switch_to(st, info, &handle, at);
            }
            if !st.can_open_more() {
                st.notice = azul_appkit::l10n::t_args(
                    "kit-doc-too-many",
                    &[("count", azul_appkit::l10n::Arg::from(docs::MAX_OPEN_DOCS))],
                );
                return Update::RefreshDom;
            }
            save_if_dirty(st, info, &handle);
            spawn(st, info, &handle, vec![storage::open_job(&id)], storage::tag::OPEN);
            Update::RefreshDom
        }
        Command::Import => {
            let _request = FileDialog::open_file(
                azul_appkit::l10n::label("azwriter-import-dialog"),
                OptionString::None,
                import_filter(),
                handle.clone(),
                on_import_picked,
            );
            Update::DoNothing
        }
        Command::Save => {
            save(st, info, &handle, storage::tag::SAVE);
            st.screen = Screen::Editor;
            Update::RefreshDom
        }
        Command::ExportPdf => export(st, info, &handle, true),
        Command::ExportMarkdown => export(st, info, &handle, false),
        Command::DeleteDocument => {
            if st.doc().is_none() {
                return Update::DoNothing;
            }
            let removed = st.open.remove(st.active);
            st.active = docs::active_after_close(st.open.len() + 1, st.active, st.active).unwrap_or(0);
            spawn(
                st,
                info,
                &handle,
                vec![storage::delete_job(&removed.id), storage::list_job()],
                storage::tag::DELETE,
            );
            st.screen = Screen::Backstage;
            st.backstage = BackstagePage::Open;
            Update::RefreshDom
        }
        Command::CloseDocument => close_document(st, info, &handle),
        Command::WordCount => {
            sync_doc(st, info);
            if let Some(doc) = st.doc() {
                st.notice = word_count_notice(doc);
            }
            Update::RefreshDom
        }
        Command::Zoom(step) => {
            st.zoom_percent = if step == 0 {
                100.0
            } else {
                (st.zoom_percent + step as f32).clamp(ZOOM_MIN, ZOOM_MAX)
            };
            Update::RefreshDom
        }
        Command::View(view) => {
            st.view = view;
            Update::RefreshDom
        }
        Command::About => {
            st.about_open = true;
            Update::RefreshDom
        }
        Command::Settings => {
            kit::open_settings(&st.kit, None);
            Update::RefreshDom
        }
    }
}

/// Runs editor commands on the shown document: a button of the ribbon or the rail took the focus,
/// it goes back into the text; the buttons that show the caret's state follow either way.
fn rich_commands(
    st: &mut AppState,
    info: &mut CallbackInfo,
    handle: &RefAny,
    commands: Vec<azul::widgets::RichTextCommand>,
) -> Update {
    let Some(doc) = st.doc_mut() else {
        return Update::DoNothing;
    };
    if flow::run(&mut doc.editor, info, commands) {
        doc.touch();
    }
    paginate::ensure(st, info, handle);
    Update::RefreshDom
}

/// Writes the shown document as PDF (`pdf`) or Markdown into the data
/// tree's exports (`writer/exports/<title>.<ext>`), on a Thread.
fn export(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, pdf: bool) -> Update {
    sync_doc(st, info);
    let Some(doc) = st.doc() else {
        return Update::DoNothing;
    };
    let (key, bytes) = if pdf {
        let Some(bytes) = pdf_bytes(doc, info) else {
            st.notice = azul_appkit::l10n::t("azwriter-pdf-failed");
            return Update::RefreshDom;
        };
        (storage::export_key(&doc.title(), "pdf"), bytes)
    } else {
        (storage::export_key(&doc.title(), "md"), doc.markdown().into_bytes())
    };
    spawn(st, info, app, vec![FileJob::Put { key, bytes }], storage::tag::EXPORT);
    Update::RefreshDom
}

/// Closes open document `index` (written first when it has changes); the one that takes the
/// screen is the neighbour that slid into its place. Closing the last document leaves the empty
/// state.
fn close_at(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, index: usize) -> Update {
    if index >= st.open.len() {
        return Update::DoNothing;
    }
    if index == st.active {
        sync_doc(st, info);
    }
    if st.open[index].is_dirty() && !st.is_saving_doc(&st.open[index].id) {
        save_at(st, info, app, index, storage::tag::SAVE);
    }
    let before = st.open.len();
    st.open.remove(index);
    st.active = docs::active_after_close(before, st.active, index).unwrap_or(0);
    st.sheet_doc = None;
    st.screen = Screen::Editor;
    Update::RefreshDom
}

/// Closes the shown document (saved first when it has changes).
fn close_document(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) -> Update {
    let index = st.active;
    close_at(st, info, app, index)
}

#[cfg(test)]
#[path = "commands_test.rs"]
mod commands_test;
