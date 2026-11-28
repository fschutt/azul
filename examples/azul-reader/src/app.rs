//! The app's state and the commands its toolbar, strip, slider and keys run.

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use azul::{callbacks::RefAny, font::FontCacheSnapshot, image::ImageRef, str::String as AzString};
use azul_appkit::pieces::ToolbarFit;

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

/// The reader's strip beside the pages: what it lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    /// Closed: the page has the whole window.
    None,
    /// The pages of the chapter on screen, as thumbnails.
    Pages,
    /// The table of contents.
    Contents,
    Bookmarks,
}

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
    /// Turns the wheel's and the trackpad's travel over the pages into page turns.
    pub wheel: position::WheelPager,
    /// The direction of the last page turn (`true` = forward): the next view slides in from
    /// below or from above and the view it replaces leaves the same way. `None` after a jump
    /// (the contents, a bookmark, a chapter), which slides nothing.
    pub turned: Option<bool>,
    /// The pictures of the pages around the view, drawn ahead on a worker.
    pub pictures: crate::pagecache::PageCache,
    /// The window's DPI factor (from the last layout): the pictures are drawn at it.
    pub dpi: f32,
    /// The clock the wheel pager reads (its start).
    pub started: std::time::Instant,
    /// A chapter being laid out: `(chapter, generation, where it opens)`.
    pub pending: Option<(usize, u64, Target)>,
    /// The counter of layouts asked for.
    pub generation: u64,
    pub pane: Pane,
    /// The user opened or closed the strip: it stays as chosen (else a desktop opens it with a
    /// book, a phone keeps it closed).
    pub pane_chosen: bool,
    /// The toolbar and the slider show over a phone's page (a tap in the middle of it).
    pub chrome: bool,
    /// The window is a phone's (a touch platform, a narrow window), from the last layout.
    pub phone: bool,
    /// The toolbar's measured fit: what the bar did with its tools at its width.
    pub toolbar_fit: ToolbarFit,
    /// The place the slider asked for last (thousandths of the book): a drag asks once a place.
    pub last_seek: Option<u32>,
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
    /// The app theme is flora or a spin of it (from the last layout): the Auto paper and the
    /// serif are flora's.
    pub flora: bool,

    // ---- the rest ----
    /// The last notice for the user ("" = none).
    pub notice: String,
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
            wheel: position::WheelPager::default(),
            turned: None,
            pictures: crate::pagecache::PageCache::default(),
            dpi: 1.0,
            started: std::time::Instant::now(),
            pending: None,
            generation: 0,
            pane: Pane::None,
            pane_chosen: false,
            chrome: false,
            phone: false,
            toolbar_fit: ToolbarFit::default(),
            last_seek: None,
            settings,
            fonts: None,
            window: (1180.0, 820.0),
            area: None,
            area_marker: azul::uuid::Uuid::short(),
            registered_images: Vec::new(),
            dark: false,
            flora: false,
            notice: String::new(),
            sample,
            sample_done: false,
            import_on_start: Vec::new(),
        }
    }

    /// The size the pages have to fit in: the measured reading area, else an estimate from the
    /// window (the toolbar and the strip taken off; on a phone nothing: they float).
    #[must_use]
    pub fn reading_area(&self) -> (f32, f32) {
        if let Some(area) = self.area {
            return area;
        }
        crate::chrome::reading_room(self.window, self.phone, self.pane != Pane::None)
    }

    /// The pages' geometry now: made for what the reading area leaves after the arrows, the
    /// gutters, the borders and the running head and the folio (a phone's margin is cut so the
    /// text keeps its width).
    #[must_use]
    pub fn geometry(&self) -> PageGeometry {
        let (w, h) = self.reading_area();
        let (across, down) = crate::chrome::page_allowance(self.phone);
        let mut settings = self.settings;
        if self.phone {
            settings.margin_px = settings.margin_px.min(crate::chrome::PHONE_MARGIN_MAX_PX);
        }
        crate::settings::page_geometry(&settings, (w - across).max(120.0), (h - down).max(200.0))
    }

    /// The layout key the current settings and area give ([`ReadingSettings::layout_key`]).
    #[must_use]
    pub fn layout_key(&self) -> String {
        let g = self.geometry();
        self.settings.layout_key(g.text_width, g.text_height, self.flora)
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

    /// What the pictures of the pages are drawn for ([`crate::pagecache::PageCache::key`]): the
    /// book, the chapter and the layout on screen, the DPI and the paper. `None` without a
    /// chapter of the open book laid out for the current settings and page size.
    #[must_use]
    pub fn pictures_key(&self) -> Option<String> {
        let open = self.open.as_ref()?;
        let chapter = self
            .chapter
            .as_ref()
            .filter(|c| c.book_id == open.info.id && c.layout_key == self.layout_key())?;
        let colors = self.settings.paper.colors(self.dark, self.flora);
        Some(format!(
            "{}/{}/{}/{}/{}/{}",
            chapter.book_id,
            chapter.chapter,
            chapter.layout_key,
            self.dpi,
            colors.background,
            colors.text
        ))
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
    /// The toolbar and the slider of a phone show or hide.
    ToggleChrome,
    /// The page field: page `n` (0-based) of the chapter on screen.
    GoToPage(usize),
    /// The slider: the book's progress (`0..=1`).
    Seek(f32),
    /// A copy of the open book's file, into the exports folder.
    Download,
    /// The chapter on screen as a PDF (to print), into the exports folder.
    Print,
    /// The page layout after the one it is on: Auto, one page, two.
    CycleLayout,
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
