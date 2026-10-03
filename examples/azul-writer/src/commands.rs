//! What the commands do. Every button carries a [`CommandRef`]
//! ([`on_command`]); keys and menus call [`run`].
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
use azul_appkit::{ui as kit, FileJob};

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

/// Folds what was typed and not reported yet into the open document.
pub fn sync_doc(st: &mut AppState, info: &mut CallbackInfo) {
    if let Some(doc) = st.doc.as_mut() {
        if doc.editor.sync(*info) {
            doc.generation = model::next_generation();
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

/// Saves the open document (the Markdown it is now) under its key; `tag`
/// says what follows the save.
pub fn save(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, tag: u64) {
    sync_doc(st, info);
    let Some(doc) = st.doc.as_ref() else {
        return;
    };
    let markdown = doc.markdown();
    let job = storage::save_job(&doc.id, &markdown);
    st.pending_saves.push((doc.id.clone(), markdown));
    spawn(st, info, app, vec![job], tag);
}

/// The open document saved first when it has changes (switching documents
/// never loses an edit).
fn save_if_dirty(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) {
    sync_doc(st, info);
    if st.is_dirty() {
        save(st, info, app, storage::tag::SAVE);
    }
}

/// `doc` becomes the open document.
pub fn show_document(st: &mut AppState, doc: DocumentModel) {
    println!("AZWRITER_OPENED {}", doc.id);
    st.doc = Some(doc);
    st.pages = paginate::Pages::default();
    st.paginating = None;
    st.screen = Screen::Editor;
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

/// The PDF of the open document (`None` without one, or when the renderer
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
        document_descriptor: AzString::from("Markdown or Word documents"),
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
    let _ = (name, result);
    Err(String::new())
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
    let path = path.as_string().as_str().to_string();
    let handle = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    // A file the user picked outside the data tree: read once, then it
    // lives in the tree like any document.
    let read = std::fs::read(&path).map_err(|e| e.to_string());
    match read.and_then(|bytes| import_bytes(&path, &bytes)) {
        Ok(doc) => {
            save_if_dirty(st, &mut info, &handle);
            show_document(st, DocumentModel::from_doc(model::new_document_id(), doc, String::new()));
            save(st, &mut info, &handle, storage::tag::SAVE);
            st.notice = format!("Imported {path}");
        }
        Err(e) => st.notice = format!("{path} could not be imported: {e}"),
    }
    Update::RefreshDom
}

/// Runs `cmd` on the app.
pub fn run(app: &mut RefAny, cmd: Command, info: &mut CallbackInfo) -> Update {
    let handle = app.clone();
    let Some(mut guard) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    match cmd {
        Command::Rich(rich) => {
            let Some(doc) = st.doc.as_mut() else {
                return Update::DoNothing;
            };
            let before = doc.editor.revision;
            let _ = doc.editor.apply_command(*info, rich);
            if doc.editor.revision != before {
                doc.generation = model::next_generation();
            }
            // The ribbon took the focus: back into the text.
            doc.editor.focus(*info);
            paginate::ensure(st, info, &handle);
            // The ribbon's pressed buttons follow either way.
            Update::RefreshDom
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
            save_if_dirty(st, info, &handle);
            show_document(st, DocumentModel::untitled(model::new_document_id()));
            Update::RefreshDom
        }
        Command::OpenDocument(index) => {
            let Some(id) = st.docs.get(index).map(|d| d.id.clone()) else {
                return Update::DoNothing;
            };
            save_if_dirty(st, info, &handle);
            spawn(st, info, &handle, vec![storage::open_job(&id)], storage::tag::OPEN);
            Update::RefreshDom
        }
        Command::Import => {
            let _request = FileDialog::open_file(
                "Import a document",
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
            let Some(doc) = st.doc.take() else {
                return Update::DoNothing;
            };
            spawn(
                st,
                info,
                &handle,
                vec![storage::delete_job(&doc.id), storage::list_job()],
                storage::tag::DELETE,
            );
            st.screen = Screen::Backstage;
            st.backstage = BackstagePage::Open;
            Update::RefreshDom
        }
        Command::CloseDocument => close_document(st, info, &handle),
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

/// Writes the open document as PDF (`pdf`) or Markdown into the data
/// tree's exports (`writer/exports/<title>.<ext>`), on a Thread.
fn export(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, pdf: bool) -> Update {
    sync_doc(st, info);
    let Some(doc) = st.doc.as_ref() else {
        return Update::DoNothing;
    };
    let (key, bytes) = if pdf {
        let Some(bytes) = pdf_bytes(doc, info) else {
            st.notice = "The PDF could not be made.".to_string();
            return Update::RefreshDom;
        };
        (storage::export_key(&doc.title(), "pdf"), bytes)
    } else {
        (storage::export_key(&doc.title(), "md"), doc.markdown().into_bytes())
    };
    spawn(st, info, app, vec![FileJob::Put { key, bytes }], storage::tag::EXPORT);
    Update::RefreshDom
}

/// Closes the open document (saved first when it has changes).
fn close_document(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) -> Update {
    save_if_dirty(st, info, app);
    st.doc = None;
    st.screen = Screen::Editor;
    Update::RefreshDom
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_of_an_import_read_is_a_document_or_a_sentence() {
        let doc = imported("notes.md", Ok(Some(b"# Hello\n\nworld\n".to_vec()))).expect("markdown");
        assert_eq!(crate::model::title_of(&doc), "Hello");
        let docx = include_bytes!("../testdata/sample.docx").to_vec();
        let doc = imported("Report.docx", Ok(Some(docx))).expect("word");
        assert_eq!(crate::model::title_of(&doc), "A Real Heading");
        assert_eq!(
            imported("gone.md", Ok(None)).err().as_deref(),
            Some("gone.md is gone.")
        );
        assert_eq!(
            imported("locked.md", Err("permission denied".to_string())).err().as_deref(),
            Some("locked.md could not be read: permission denied")
        );
        let broken = imported("broken.docx", Ok(Some(b"not a zip".to_vec()))).expect_err("not Word");
        assert!(broken.starts_with("broken.docx could not be imported: "), "{broken}");
    }

    #[test]
    fn an_import_reads_word_by_name_and_markdown_otherwise() {
        let doc = import_bytes("notes.md", b"# Hello\n\nworld\n").expect("markdown");
        assert_eq!(crate::model::title_of(&doc), "Hello");
        let docx = include_bytes!("../testdata/sample.docx");
        let doc = import_bytes("Report.DOCX", docx).expect("word");
        assert_eq!(crate::model::title_of(&doc), "A Real Heading");
    }
}
