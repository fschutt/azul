//! File: the backstage. Info (the open document: title, words, pages,
//! where its file is; Save, Export, Delete), New (a blank document, an
//! import), Open (the documents in the data tree), Save, Export (PDF and
//! Markdown into the data tree's exports), Close.

use azul::{
    callbacks::{BackstageOnNavSelectCallbackType, ButtonOnClickCallbackType, CallbackInfo, RefAny, Update},
    dom::Dom,
    str::String as AzString,
    widgets::{Backstage, BackstageNavItem, Button, ButtonType},
};

use crate::{
    app::{command, AppState, BackstagePage, Command},
    commands::on_command,
    ids, storage,
};

/// A key of the resources said, or words as they are (a document's title).
fn s(text: &str) -> AzString {
    azul_appkit::l10n::label(text)
}

fn button(app: &RefAny, id: AzString, label: &str, cmd: Command) -> Dom {
    Button::create(s(label))
        .with_on_click(command(app, cmd), on_command as ButtonOnClickCallbackType)
        .dom()
        .with_id(id)
}

fn primary(app: &RefAny, id: AzString, label: &str, cmd: Command) -> Dom {
    Button::create(s(label))
        .with_button_type(ButtonType::Primary)
        .with_on_click(command(app, cmd), on_command as ButtonOnClickCallbackType)
        .dom()
        .with_id(id)
}

fn heading(text: &str) -> Dom {
    Dom::create_h2_with_text(s(text))
        .with_css("margin: 0px 0px 16px 0px; font-size: 26px; font-weight: normal;")
}

fn line(text: &str) -> Dom {
    Dom::create_p_with_text(s(text)).with_css("margin: 0px 0px 8px 0px; font-size: 13px;")
}

fn buttons(children: Vec<Dom>) -> Dom {
    let mut row = Dom::create_div()
        .with_css("display: flex; flex-direction: row; flex-wrap: wrap; margin-top: 12px;");
    for child in children {
        row.add_child(Dom::create_div().with_css("margin-right: 8px; margin-bottom: 8px;").with_child(child));
    }
    row
}

fn pane(children: Vec<Dom>) -> Dom {
    let mut pane = Dom::create_div().with_css(
        "display: flex; flex-direction: column; flex-grow: 1; padding: 24px 36px; overflow-y: auto;",
    );
    for child in children {
        pane.add_child(child);
    }
    pane
}

/// Where the open document's file is, as the user reads it.
fn file_line(st: &AppState) -> String {
    match st.doc.as_ref() {
        Some(doc) => azul_appkit::l10n::t_args(
            "azwriter-file",
            &[(
                "path",
                azul_appkit::l10n::Arg::from(
                    azul_appkit::data::local_path(&st.data_root, &doc.key()).display().to_string(),
                ),
            )],
        ),
        None => azul_appkit::l10n::t("azwriter-no-doc-open"),
    }
}

fn info_page(app: &RefAny, st: &AppState) -> Dom {
    let Some(doc) = st.doc.as_ref() else {
        return pane(vec![
            heading("azwriter-page-info"),
            line("azwriter-no-doc-open"),
            buttons(vec![primary(app, ids::NEW_DOC, "azwriter-blank-document", Command::NewDocument)]),
        ]);
    };
    let pages = st.pages.starts_for(doc.doc().block_count()).len();
    pane(vec![
        heading(&doc.title()),
        line(&azul_appkit::l10n::t_args(
            if doc.is_dirty() { "azwriter-info-counts-unsaved" } else { "azwriter-info-counts" },
            &[
                ("words", azul_appkit::l10n::Arg::from(doc.word_count())),
                ("pages", azul_appkit::l10n::Arg::from(pages)),
            ],
        )),
        line(&file_line(st)),
        buttons(vec![
            primary(app, ids::SAVE, "azwriter-page-save", Command::Save),
            button(app, ids::EXPORT_PDF, "azwriter-export-pdf", Command::ExportPdf),
            button(app, ids::DELETE, "azwriter-delete", Command::DeleteDocument),
        ]),
    ])
}

fn new_page(app: &RefAny) -> Dom {
    pane(vec![
        heading("azwriter-page-new"),
        line("azwriter-new-what"),
        buttons(vec![
            primary(app, ids::NEW_DOC, "azwriter-blank-document", Command::NewDocument),
            button(app, ids::IMPORT, "azwriter-import-file", Command::Import),
        ]),
    ])
}

fn open_page(app: &RefAny, st: &AppState) -> Dom {
    let mut list = Dom::create_div()
        .with_id(ids::DOC_LIST)
        .with_css("display: flex; flex-direction: column; margin-top: 8px;");
    if st.listed && st.docs.is_empty() {
        list.add_child(line("azwriter-no-documents"));
    }
    for (i, entry) in st.docs.iter().enumerate() {
        let label = azul_appkit::l10n::t_args(
            "azwriter-doc-row",
            &[
                ("title", azul_appkit::l10n::Arg::from(entry.title.as_str())),
                ("words", azul_appkit::l10n::Arg::from(entry.words)),
            ],
        );
        list.add_child(
            Dom::create_div()
                .with_css("margin-bottom: 6px;")
                .with_child(button(
                    app,
                    s(&format!("{}{i}", ids::OPEN_ROW)),
                    &label,
                    Command::OpenDocument(i),
                )),
        );
    }
    pane(vec![
        heading("azwriter-page-open"),
        line(&azul_appkit::l10n::t_args(
            "azwriter-your-documents",
            &[(
                "folder",
                azul_appkit::l10n::Arg::from(
                    azul_appkit::data::local_path(&st.data_root, storage::APP_FOLDER).display().to_string(),
                ),
            )],
        )),
        list,
    ])
}

fn save_page(app: &RefAny, st: &AppState) -> Dom {
    pane(vec![
        heading("azwriter-page-save"),
        line("azwriter-save-what"),
        line(&file_line(st)),
        buttons(vec![primary(app, ids::SAVE, "azwriter-page-save", Command::Save)]),
    ])
}

fn export_page(app: &RefAny, st: &AppState) -> Dom {
    let folder = azul_appkit::data::local_path(
        &st.data_root,
        &azul_appkit::data::app_key(storage::APP_FOLDER, "exports"),
    );
    pane(vec![
        heading("azwriter-page-export"),
        line("azwriter-export-what"),
        line(&azul_appkit::l10n::t_args(
            "azwriter-exports-go-to",
            &[("folder", azul_appkit::l10n::Arg::from(folder.display().to_string()))],
        )),
        buttons(vec![
            primary(app, ids::EXPORT_PDF, "azwriter-export-pdf", Command::ExportPdf),
            button(app, ids::EXPORT_MD, "azwriter-export-markdown", Command::ExportMarkdown),
        ]),
    ])
}

extern "C" fn on_nav(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let page = BackstagePage::NAV.get(index).copied().unwrap_or(BackstagePage::Info);
    crate::commands::run(&mut data, Command::OpenBackstage(page), &mut info)
}

/// The backstage for the app's state.
#[must_use]
pub fn backstage(app: &RefAny, st: &AppState) -> Dom {
    let content = match st.backstage {
        BackstagePage::Info | BackstagePage::Close => info_page(app, st),
        BackstagePage::New => new_page(app),
        BackstagePage::Open => open_page(app, st),
        BackstagePage::Save => save_page(app, st),
        BackstagePage::Export => export_page(app, st),
    };
    let nav: Vec<BackstageNavItem> = BackstagePage::NAV
        .iter()
        .map(|p| {
            let item = BackstageNavItem::create(s(p.label()));
            if *p == BackstagePage::Close {
                item.with_gap_before()
            } else {
                item
            }
        })
        .collect();
    Backstage::create(nav)
        .with_active_item(st.backstage.index())
        .with_on_nav_select(app.clone(), on_nav as BackstageOnNavSelectCallbackType)
        .with_on_back(command(app, Command::CloseBackstage), on_command as ButtonOnClickCallbackType)
        .with_content(content)
        .dom()
}
