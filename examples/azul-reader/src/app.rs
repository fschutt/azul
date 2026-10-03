//! The app's state and the commands its ribbon, panes, status bar and keys run.

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use azul::{callbacks::RefAny, font::FontCacheSnapshot, image::ImageRef, str::String as AzString};

use crate::{
    epub::{Book, Container},
    library::{BookInfo, BookState, LibraryEntry, Shelf, Sort},
    paginate::ChapterReady,
    position::{self, Position},
    settings::{FontChoice, PageGeometry, PageLayout, Paper, ReadingSettings},
};

/// What the window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// The covers grid.
    Library,
    /// The pages of the open book.
    Reader,
}

/// The reader's side pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    None,
    Contents,
    Bookmarks,
}

/// The ribbon's tabs (the library's, then the reader's).
pub const LIBRARY_TABS: [&str; 2] = ["HOME", "VIEW"];
pub const READER_TABS: [&str; 2] = ["READ", "VIEW"];

/// Where a chapter being laid out opens.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// The page this share of the chapter's height is on.
    Fraction(f32),
    /// The page the element with this id is on.
    Anchor(String),
    /// The chapter's last page.
    LastPage,
}

/// The book open in the reader.
pub struct OpenBook {
    pub info: BookInfo,
    pub state: BookState,
    pub container: Arc<Container>,
    pub book: Book,
    /// Every chapter's weight in the book's progress.
    pub weights: Vec<u64>,
}

impl OpenBook {
    /// The book's id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.info.id
    }
}

/// Everything AzReader knows.
pub struct AppState {
    /// azul-appkit's kit: settings, data root, the settings page.
    pub kit: RefAny,
    /// The data root (the bucket's root, later).
    pub data_root: PathBuf,
    pub screen: Screen,
    pub ribbon_tab: usize,

    // ---- the library ----
    pub entries: Vec<LibraryEntry>,
    /// The covers by book id.
    pub covers: BTreeMap<String, ImageRef>,
    /// The library was read at least once.
    pub listed: bool,
    pub shelf: Shelf,
    pub sort: Sort,
    pub query: String,
    /// The selected book (its id).
    pub selected: Option<String>,

    // ---- the reader ----
    pub open: Option<OpenBook>,
    /// A book being opened (its id).
    pub opening: Option<String>,
    /// The chapter on screen, laid out for the current settings and page size.
    pub chapter: Option<ChapterReady>,
    /// The first page of the view in that chapter.
    pub page: usize,
    /// A chapter being laid out: `(chapter, generation, where it opens)`.
    pub pending: Option<(usize, u64, Target)>,
    /// The counter of layouts asked for.
    pub generation: u64,
    pub pane: Pane,
    pub settings: ReadingSettings,
    /// The window's fonts, for the pagination thread (taken in `layout`).
    pub fonts: Option<FontCacheSnapshot>,
    /// The window's size (logical px), from the last layout.
    pub window: (f32, f32),
    /// The reading area's size as laid out (measured after a frame; `None` = not yet).
    pub area: Option<(f32, f32)>,
    /// The marker that finds the reading area's node.
    pub area_marker: AzString,
    /// The image-cache names the open chapter registered (removed when it goes).
    pub registered_images: Vec<String>,
    /// The window is in the dark mode (from the last layout).
    pub dark: bool,

    // ---- the rest ----
    /// The last notice for the user ("" = none).
    pub notice: String,
    pub about_open: bool,
    /// Fill an empty library with the sample book (`--sample`), once.
    pub sample: bool,
    pub sample_done: bool,
    /// Files named on the command line, imported once the window is up.
    pub import_on_start: Vec<PathBuf>,
}

impl AppState {
    /// A fresh state for the kit's data root.
    #[must_use]
    pub fn new(kit: RefAny, data_root: PathBuf, settings: ReadingSettings, sample: bool) -> Self {
        Self {
            kit,
            data_root,
            screen: Screen::Library,
            ribbon_tab: 0,
            entries: Vec::new(),
            covers: BTreeMap::new(),
            listed: false,
            shelf: Shelf::All,
            sort: Sort::Recent,
            query: String::new(),
            selected: None,
            open: None,
            opening: None,
            chapter: None,
            page: 0,
            pending: None,
            generation: 0,
            pane: Pane::None,
            settings,
            fonts: None,
            window: (1180.0, 820.0),
            area: None,
            area_marker: azul::uuid::Uuid::short(),
            registered_images: Vec::new(),
            dark: false,
            notice: String::new(),
            about_open: false,
            sample,
            sample_done: false,
            import_on_start: Vec::new(),
        }
    }

    /// The size the pages have to fit in: the measured reading area, else an estimate from the
    /// window (the title row, the ribbon, the status bar and the side pane taken off).
    #[must_use]
    pub fn reading_area(&self) -> (f32, f32) {
        if let Some(area) = self.area {
            return area;
        }
        let pane = if self.pane == Pane::None { 0.0 } else { 280.0 };
        (
            (self.window.0 - pane).max(200.0),
            (self.window.1 - 190.0).max(200.0),
        )
    }

    /// The pages' geometry now.
    #[must_use]
    pub fn geometry(&self) -> PageGeometry {
        let (w, h) = self.reading_area();
        // The running head and the folio take a line each above and below the pages.
        crate::settings::page_geometry(&self.settings, w, (h - 48.0).max(200.0))
    }

    /// The layout key the current settings and area give ([`ReadingSettings::layout_key`]).
    #[must_use]
    pub fn layout_key(&self) -> String {
        let g = self.geometry();
        self.settings.layout_key(g.text_width, g.text_height)
    }

    /// Where the reader is (the open chapter's view start; the saved position while the
    /// chapter is laid out).
    #[must_use]
    pub fn position(&self) -> Option<Position> {
        let open = self.open.as_ref()?;
        match self.chapter.as_ref() {
            Some(ch) if ch.book_id == open.info.id => Some(Position {
                chapter: ch.chapter,
                fraction: ch.pages.fraction_of_page(self.page),
            }),
            _ => Some(open.state.position),
        }
    }

    /// The book's progress now (`0..=1`).
    #[must_use]
    pub fn progress(&self) -> f32 {
        match (self.open.as_ref(), self.position()) {
            (Some(open), Some(at)) => position::book_progress(&open.weights, at),
            _ => 0.0,
        }
    }

    /// The window's title.
    #[must_use]
    pub fn title(&self) -> String {
        match (self.screen, self.open.as_ref()) {
            (Screen::Reader, Some(open)) => format!("{} - AzReader", open.info.title),
            _ => "AzReader".to_string(),
        }
    }
}

/// What a button, a key or a pane asks of the app.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    RibbonTab(usize),
    ShowLibrary,
    /// Back to the open book.
    ShowReader,
    Shelf(Shelf),
    Sort(Sort),
    Select(String),
    OpenBook(String),
    RemoveBook(String),
    /// Asks for files to add (the file dialog).
    AddBooks,
    NextPage,
    PrevPage,
    /// The first page of the book / the last page.
    BookStart,
    /// Entry `index` of the table of contents.
    GoToEntry(usize),
    /// Bookmark `index`.
    GoToBookmark(usize),
    ToggleBookmark,
    RemoveBookmark(usize),
    Pane(Pane),
    /// The font size by this many px.
    FontSize(i32),
    /// The line spacing by this many tenths.
    LineSpacing(i32),
    /// The margins by this many px.
    Margins(i32),
    Paper(Paper),
    Font(FontChoice),
    Layout(PageLayout),
    Justify(bool),
    Settings,
    About,
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
