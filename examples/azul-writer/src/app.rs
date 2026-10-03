//! The app's state and the commands its ribbon, backstage, status bar and
//! keys run.

use std::path::PathBuf;

use azul::{
    callbacks::RefAny,
    font::FontCacheSnapshot,
    str::String as AzString,
    widgets::{RichAlign, RichBlockKind, RichFormat, RichTextCommand},
};

use crate::{model::DocumentModel, paginate::Pages, storage::DocEntry};

/// What the window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// The ribbon, the pages, the status bar.
    Editor,
    /// File: the backstage.
    Backstage,
}

/// A page of the backstage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackstagePage {
    Info,
    New,
    Open,
    Save,
    Export,
    Close,
}

impl BackstagePage {
    /// The pages in the navigation's order.
    pub const NAV: [BackstagePage; 6] = [
        BackstagePage::Info,
        BackstagePage::New,
        BackstagePage::Open,
        BackstagePage::Save,
        BackstagePage::Export,
        BackstagePage::Close,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            BackstagePage::Info => "Info",
            BackstagePage::New => "New",
            BackstagePage::Open => "Open",
            BackstagePage::Save => "Save",
            BackstagePage::Export => "Export",
            BackstagePage::Close => "Close",
        }
    }

    #[must_use]
    pub fn index(self) -> usize {
        Self::NAV.iter().position(|p| *p == self).unwrap_or(0)
    }
}

/// How the document is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// A4 sheets, as printed.
    Print,
    /// One continuous sheet as wide as the window allows (no pages).
    Web,
}

impl View {
    pub const ALL: [View; 2] = [View::Print, View::Web];

    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            View::Print => "description",
            View::Web => "public",
        }
    }

    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|v| *v == self).unwrap_or(0)
    }
}

/// The zoom range of the status bar (percent).
pub const ZOOM_MIN: f32 = 50.0;
pub const ZOOM_MAX: f32 = 200.0;

/// The ribbon's tabs.
pub const TABS: [&str; 3] = ["HOME", "INSERT", "VIEW"];

/// Everything AzWriter knows.
pub struct AppState {
    /// azul-appkit's kit: settings, data root, the settings page.
    pub kit: RefAny,
    /// The data root (the bucket's root, later).
    pub data_root: PathBuf,
    pub screen: Screen,
    pub backstage: BackstagePage,
    pub ribbon_tab: usize,
    /// The open document (`None`: the empty state).
    pub doc: Option<DocumentModel>,
    /// The documents in the data tree (the backstage's Open page).
    pub docs: Vec<DocEntry>,
    /// The list was read at least once.
    pub listed: bool,
    /// Where the pages start (for the open document's last pagination).
    pub pages: Pages,
    /// The generation a pagination thread is working on.
    pub paginating: Option<u64>,
    /// The window's fonts, for the pagination thread (taken in `layout`).
    pub fonts: Option<FontCacheSnapshot>,
    pub zoom_percent: f32,
    pub view: View,
    /// The last notice for the user ("" = none).
    pub notice: String,
    /// The saves on the way: `(document id, the Markdown it writes)`; a
    /// document is clean at that text once its save answers.
    pub pending_saves: Vec<(String, String)>,
    /// The sample document was made (`--sample`, once).
    pub sample_done: bool,
    /// A document imported at start (a file on the command line) is saved
    /// into the data tree once the window is up.
    pub save_on_start: bool,
    /// The window closes once the save on the way is done.
    pub close_after_save: bool,
    /// The "Save changes?" question is showing.
    pub asking_close: bool,
    pub about_open: bool,
    /// Fill an empty data folder with the sample document (`--sample`).
    pub sample: bool,
    /// The status bar's word count segment (updated while typing).
    pub word_count_marker: AzString,
}

impl AppState {
    /// A fresh state for the kit's data root.
    #[must_use]
    pub fn new(kit: RefAny, data_root: PathBuf, sample: bool) -> Self {
        Self {
            kit,
            data_root,
            screen: Screen::Editor,
            backstage: BackstagePage::Info,
            ribbon_tab: 0,
            doc: None,
            docs: Vec::new(),
            listed: false,
            pages: Pages::default(),
            paginating: None,
            fonts: None,
            zoom_percent: 100.0,
            view: View::Print,
            notice: String::new(),
            pending_saves: Vec::new(),
            sample_done: false,
            save_on_start: false,
            close_after_save: false,
            asking_close: false,
            about_open: false,
            sample,
            word_count_marker: azul::uuid::Uuid::short(),
        }
    }

    /// A save is on the way.
    #[must_use]
    pub fn is_saving(&self) -> bool {
        !self.pending_saves.is_empty()
    }

    /// The open document has changes its file does not.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.doc.as_ref().is_some_and(DocumentModel::is_dirty)
    }

    /// The window's title: the document's, a star while unsaved.
    #[must_use]
    pub fn title(&self) -> String {
        match self.doc.as_ref() {
            Some(doc) => format!(
                "{}{} - AzWriter",
                doc.title(),
                if doc.is_dirty() { " *" } else { "" }
            ),
            None => "AzWriter".to_string(),
        }
    }
}

/// What a button, a key or a menu asks of the app.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// A command of the shared editor on the open document (formats,
    /// block kinds, alignment, lists, undo / redo, inserts).
    Rich(RichTextCommand),
    RibbonTab(usize),
    OpenBackstage(BackstagePage),
    CloseBackstage,
    NewDocument,
    /// Opens the document at this index of the backstage's list.
    OpenDocument(usize),
    /// Asks for a Markdown or Word file to import as a new document.
    Import,
    Save,
    ExportPdf,
    ExportMarkdown,
    /// Deletes the open document's file (the document closes).
    DeleteDocument,
    CloseDocument,
    /// The zoom by this many percent.
    Zoom(i32),
    View(View),
    About,
    Settings,
}

impl Command {
    /// The editor's format toggle.
    #[must_use]
    pub fn format(format: RichFormat) -> Self {
        Command::Rich(RichTextCommand::ToggleFormat(format))
    }

    /// The editor's block kind.
    #[must_use]
    pub fn kind(kind: RichBlockKind) -> Self {
        Command::Rich(RichTextCommand::ToggleKind(kind))
    }

    /// The editor's alignment.
    #[must_use]
    pub fn align(align: RichAlign) -> Self {
        Command::Rich(RichTextCommand::SetAlign(align))
    }
}

/// The payload of a button that runs `cmd`.
pub struct CommandRef {
    pub app: RefAny,
    pub cmd: Command,
}

/// The `RefAny` a button carries to run `cmd` on `app`.
#[must_use]
pub fn command(app: &RefAny, cmd: Command) -> RefAny {
    RefAny::new(CommandRef {
        app: app.clone(),
        cmd,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backstage_pages_and_the_views_are_found_by_index() {
        for (i, page) in BackstagePage::NAV.iter().enumerate() {
            assert_eq!(page.index(), i);
        }
        assert_eq!(View::Web.index(), 1);
    }
}
