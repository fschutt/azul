//! The app's state and the commands its ribbon, backstage, sidebar, status bar and
//! keys run.

use std::path::PathBuf;

use azul::{
    callbacks::RefAny,
    font::FontCacheSnapshot,
    str::String as AzString,
    widgets::{RichAlign, RichBlockKind, RichFormat, RichTextCommand, SidebarState},
};
use azul_appkit::{
    docs::{self, DocRow, DocSync},
    flow_rules::Quick,
};

use crate::{model::DocumentModel, storage::DocEntry};

/// What the window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// The sidebar, the ribbon, the text, the status bar.
    Editor,
    /// File's old pages (`...` of the location, "New or open"): the backstage.
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

/// The backstage's pages in the navigation's order.
pub static BACKSTAGE_NAV: [BackstagePage; 6] = [
    BackstagePage::Info,
    BackstagePage::New,
    BackstagePage::Open,
    BackstagePage::Save,
    BackstagePage::Export,
    BackstagePage::Close,
];

impl BackstagePage {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            BackstagePage::Info => "azwriter-page-info",
            BackstagePage::New => "azwriter-page-new",
            BackstagePage::Open => "azwriter-page-open",
            BackstagePage::Save => "azwriter-page-save",
            BackstagePage::Export => "azwriter-page-export",
            BackstagePage::Close => "azwriter-page-close",
        }
    }

    #[must_use]
    pub fn index(self) -> usize {
        BACKSTAGE_NAV.iter().position(|p| *p == self).unwrap_or(0)
    }
}

/// How the document is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    /// The text FLOWS: one continuous column as wide as is comfortable, like a mobile Markdown
    /// editor. The default, and the only layout a phone has.
    #[default]
    Flow,
    /// A4 sheets, as printed: the page layout (a View option; a window narrower than a sheet shows
    /// the flow instead).
    Pages,
}

/// Both views, in the order of the View tab and the status bar's switcher.
pub static VIEWS: [View; 2] = [View::Flow, View::Pages];

impl View {
    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            View::Flow => "notes",
            View::Pages => "description",
        }
    }

    #[must_use]
    pub fn index(self) -> usize {
        VIEWS.iter().position(|v| *v == self).unwrap_or(0)
    }
}

/// The zoom range of the status bar (percent).
pub const ZOOM_MIN: f32 = 50.0;
pub const ZOOM_MAX: f32 = 200.0;

/// The ribbon's tabs (keys of the resources): File shows the location, the rest are bands.
pub static TABS: [&str; 6] = [
    "azwriter-tab-file",
    "azwriter-tab-home",
    "azwriter-tab-insert",
    "azwriter-tab-layout",
    "azwriter-tab-review",
    "azwriter-tab-view",
];

/// The icons of the tabs, where a phone's pills are icons (the same order as [`TABS`]).
pub static TAB_ICONS: [&str; 6] = ["folder", "home", "add_circle", "view_quilt", "rate_review", "visibility"];

/// The tab the window starts on: File, the location of the document (the wireframe's first tab).
pub const START_TAB: usize = 0;

/// Everything AzWriter knows.
pub struct AppState {
    /// azul-appkit's kit: settings, data root, the settings page.
    pub kit: RefAny,
    /// The data root (the bucket's root, later).
    pub data_root: PathBuf,
    pub screen: Screen,
    pub backstage: BackstagePage,
    pub ribbon_tab: usize,
    /// The open documents, in the order of the sidebar.
    pub open: Vec<DocumentModel>,
    /// Which of them the main view shows (an index into `open`).
    pub active: usize,
    /// The documents in the data tree (the backstage's Open page).
    pub docs: Vec<DocEntry>,
    /// The list was read at least once.
    pub listed: bool,
    /// The window's fonts, for the pagination thread (taken in `layout`).
    pub fonts: Option<FontCacheSnapshot>,
    pub zoom_percent: f32,
    pub view: View,
    /// The state of the sidebar: the rail (a phone starts there), expanded (a desktop starts so).
    pub sidebar: SidebarState,
    /// Whether the last layout was a phone's (an event handler asks what the layout decided).
    pub phone: bool,
    /// The first layout chose the sidebar's starting state (the rail on a phone, open on a desktop).
    pub sidebar_chosen: bool,
    /// The open document whose action sheet shows (a phone's long press on its row).
    pub sheet_doc: Option<usize>,
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
    /// A fresh state for the kit's data root, no document open yet.
    #[must_use]
    pub fn new(kit: RefAny, data_root: PathBuf, sample: bool) -> Self {
        Self {
            kit,
            data_root,
            screen: Screen::Editor,
            backstage: BackstagePage::Info,
            ribbon_tab: START_TAB,
            open: Vec::new(),
            active: 0,
            docs: Vec::new(),
            listed: false,
            fonts: None,
            zoom_percent: 100.0,
            view: View::Flow,
            sidebar: SidebarState::Rail,
            phone: false,
            sidebar_chosen: false,
            sheet_doc: None,
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

    /// The document the main view shows (`None`: nothing is open).
    #[must_use]
    pub fn doc(&self) -> Option<&DocumentModel> {
        self.open.get(self.active)
    }

    /// [`Self::doc`], to change it.
    pub fn doc_mut(&mut self) -> Option<&mut DocumentModel> {
        self.open.get_mut(self.active)
    }

    /// The open document with this id.
    #[must_use]
    pub fn doc_by_id(&self, id: &str) -> Option<&DocumentModel> {
        self.open.iter().find(|d| d.id == id)
    }

    /// The index of the open document with this id.
    #[must_use]
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.open.iter().position(|d| d.id == id)
    }

    /// A save is on the way.
    #[must_use]
    pub fn is_saving(&self) -> bool {
        !self.pending_saves.is_empty()
    }

    /// A save of document `id` is on the way.
    #[must_use]
    pub fn is_saving_doc(&self, id: &str) -> bool {
        self.pending_saves.iter().any(|(i, _)| i == id)
    }

    /// The shown document has changes its file does not.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.doc().is_some_and(DocumentModel::is_dirty)
    }

    /// Some open document has changes its file does not (what the close guard asks about).
    #[must_use]
    pub fn any_dirty(&self) -> bool {
        self.open.iter().any(DocumentModel::is_dirty)
    }

    /// Where the save of open document `index` stands, as its sidebar pie says it.
    #[must_use]
    pub fn sync_of(&self, index: usize) -> DocSync {
        match self.open.get(index) {
            Some(doc) => DocSync::of(doc.is_dirty(), self.is_saving_doc(&doc.id), doc.save_failed),
            None => DocSync::Saved,
        }
    }

    /// The rows of the open documents for the sidebar (`docs::DocRow`), the shown one selected.
    #[must_use]
    pub fn doc_rows(&self) -> Vec<DocRow> {
        self.open
            .iter()
            .enumerate()
            .map(|(index, doc)| DocRow {
                name: doc.title(),
                sync: self.sync_of(index),
                collab: doc.collab,
                selected: index == self.active,
            })
            .collect()
    }

    /// Whether one more document may be opened: [`docs::MAX_OPEN_DOCS`] is the most.
    #[must_use]
    pub fn can_open_more(&self) -> bool {
        self.open.len() < docs::MAX_OPEN_DOCS
    }

    /// "Don't Save" in the close question: the documents with changes close without
    /// writing, their files keep what they had. The guard closes the window
    /// right after and judges that close by the DOM rebuilt from this
    /// state, so nothing may be left to save here.
    pub fn discard_document(&mut self) {
        self.open.retain(|d| !d.is_dirty());
        self.active = self.active.min(self.open.len().saturating_sub(1));
        self.asking_close = false;
    }

    /// The window's title: the document's, a star while unsaved.
    #[must_use]
    pub fn title(&self) -> String {
        match self.doc() {
            Some(doc) => format!(
                "{}{} - AzWriter",
                doc.title(),
                if doc.is_dirty() { " *" } else { "" }
            ),
            None => "AzWriter".to_string(),
        }
    }
}

/// What a button, a key, a menu or the sidebar asks of the app.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// A command of the shared editor on the shown document (formats,
    /// block kinds, alignment, lists, undo / redo, inserts).
    Rich(RichTextCommand),
    /// A formatting button of the sidebar's rail.
    Quick(Quick),
    RibbonTab(usize),
    OpenBackstage(BackstagePage),
    CloseBackstage,
    NewDocument,
    /// Shows open document `index`.
    Switch(usize),
    /// Closes open document `index` (saved first when it has changes).
    Close(usize),
    /// Switches collaboration on or off for open document `index` (UI only).
    Collab(usize, bool),
    /// Opens the document at this index of the backstage's list.
    OpenDocument(usize),
    /// Asks for a Markdown or Word file to import as a new document.
    Import,
    Save,
    ExportPdf,
    ExportMarkdown,
    /// Deletes the shown document's file (the document closes).
    DeleteDocument,
    CloseDocument,
    /// Says how many words, characters and pages the shown document has.
    WordCount,
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
#[path = "app_test.rs"]
mod app_test;
