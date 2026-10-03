//! The app's state: the editing session (if a deck is open), the screen and
//! view, the backstage page, the canvas's zoom and its transient drawing
//! (snap guides, marquee), the pictures, the slide show in flight; and the
//! [`Command`]s the ribbon, the backstage, the menus and the keys run.

use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Instant};

use azul::{
    callbacks::RefAny,
    image::ImageRef,
    widgets::{AdornerFrame, AdornerGuide},
};
use azul_storage::{Drive, LocalDrive};

use crate::{
    args::Args,
    editor::Editor,
    model::{
        Align, AnimationEffect, Background, Blank, ChartKind, Color, ImageFit, LayoutKind, ShapeKind,
        ShowState, SlideSize, TransitionKind, ZOrder,
    },
    storage::DeckSummary,
};

/// What the editor area shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// The rail, the slide, the notes.
    Normal,
    /// The slide sorter.
    Sorter,
    /// The outline.
    Outline,
    /// The notes page.
    NotesPage,
}

impl View {
    /// The status bar's view buttons, in order.
    pub const ALL: [View; 4] = [View::Normal, View::Sorter, View::Outline, View::NotesPage];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            View::Normal => "Normal",
            View::Sorter => "Slide Sorter",
            View::Outline => "Outline View",
            View::NotesPage => "Notes Page",
        }
    }

    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            View::Normal => "view_compact",
            View::Sorter => "grid_view",
            View::Outline => "format_list_bulleted",
            View::NotesPage => "sticky_note_2",
        }
    }

    #[must_use]
    pub fn index(self) -> usize {
        View::ALL.iter().position(|v| *v == self).unwrap_or(0)
    }
}

/// The backstage's pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackstagePage {
    Info,
    New,
    Open,
    Save,
    Export,
    Close,
    Options,
    About,
}

impl BackstagePage {
    /// The navigation, top to bottom.
    pub const NAV: [BackstagePage; 8] = [
        BackstagePage::Info,
        BackstagePage::New,
        BackstagePage::Open,
        BackstagePage::Save,
        BackstagePage::Export,
        BackstagePage::Close,
        BackstagePage::Options,
        BackstagePage::About,
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
            BackstagePage::Options => "Options",
            BackstagePage::About => "About",
        }
    }

    #[must_use]
    pub fn index(self) -> usize {
        BackstagePage::NAV.iter().position(|p| *p == self).unwrap_or(0)
    }
}

/// The window's screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// The ribbon and a view.
    Editor,
    /// File: the backstage.
    Backstage,
    /// The slide show (the presenter view is a second window).
    Show,
}

/// A build or a transition playing in the show.
#[derive(Debug, Clone)]
pub struct Play {
    /// The elements whose build plays (empty for a transition).
    pub ids: Vec<u64>,
    /// A transition from this slide to the current one.
    pub transition_from: Option<usize>,
    pub started: Instant,
    pub duration_ms: u32,
}

impl Play {
    /// 0..1, how far it has played.
    #[must_use]
    pub fn progress(&self) -> f32 {
        let ms = self.started.elapsed().as_secs_f32() * 1000.0;
        (ms / self.duration_ms.max(1) as f32).clamp(0.0, 1.0)
    }

    #[must_use]
    pub fn done(&self) -> bool {
        self.progress() >= 1.0
    }
}

/// The slide show in flight.
#[derive(Debug, Clone)]
pub struct ShowRuntime {
    pub state: ShowState,
    pub started: Instant,
    /// The presenter window is open.
    pub presenter: bool,
    pub play: Option<Play>,
    /// Digits typed for "number + Enter".
    pub typed: String,
    /// The view to go back to.
    pub return_view: View,
}

/// Where the presenter view opens (PowerPoint's Slide Show > Monitors): on a
/// screen of its own when there is one, or on the monitor the user picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PresenterMonitor {
    #[default]
    Automatic,
    /// The monitor with this index (`Monitor::monitor_id.index`).
    Monitor(u32),
}

impl PresenterMonitor {
    /// The key of the choice in the kit's settings.json.
    pub const SETTING: &'static str = "presenter_monitor";

    /// The choice a settings value names: a monitor index, else automatic.
    #[must_use]
    pub fn parse(value: Option<&str>) -> Self {
        let _ = value;
        PresenterMonitor::Automatic
    }

    /// The settings value of the choice.
    #[must_use]
    pub fn to_setting(self) -> String {
        String::new()
    }

    /// The monitor the presenter window opens on, among the connected
    /// `monitors` (their indices) while the show runs on `show`: the chosen
    /// one while it is connected, else the first one that is not the show's;
    /// `None` (the system's choice) with one screen.
    #[must_use]
    pub fn resolve(self, monitors: &[u32], show: Option<u32>) -> Option<u32> {
        let _ = (monitors, show);
        None
    }
}

/// Everything the app holds.
pub struct AppState {
    pub editor: Option<Editor>,
    pub screen: Screen,
    pub view: View,
    pub page: BackstagePage,
    pub ribbon_tab: usize,
    /// The canvas's zoom in percent; `None` fits the slide to the window.
    pub zoom: Option<f32>,
    pub show_notes: bool,
    /// The snap guides of the drag in flight, handed back to the adorner.
    pub guides: Vec<AdornerGuide>,
    /// The marquee of the drag in flight.
    pub marquee: Option<AdornerFrame>,
    /// The open deck's pictures, decoded, by media key.
    pub media: HashMap<String, ImageRef>,
    /// The decks File > Open lists.
    pub decks: Vec<DeckSummary>,
    /// File > New: the theme, its colour variant, the font scheme, the size.
    pub new_theme: usize,
    pub new_variant: usize,
    pub new_fonts: usize,
    pub new_size: SlideSize,
    pub show: Option<ShowRuntime>,
    /// `<data root>`: decks are under `<data root>/show/` (for the user's
    /// eyes: files go through `drive`).
    pub data_root: PathBuf,
    /// The ONE drive every storage job goes through - a `LocalDrive` on
    /// `data_root` today, an `S3Drive` later with no other change
    /// (DEDUP_OFFICE D21: a root path per job was an S3 blocker).
    pub drive: Arc<dyn Drive>,
    /// azul-appkit's kit (settings, data root, shortcuts, About); `None` in
    /// the unit tests.
    pub kit: Option<RefAny>,
    /// The status bar's last message ("Saved", an error).
    pub message: String,
    /// Storage jobs in flight.
    pub busy: usize,
    pub args: Args,
    /// The element whose text gets the focus after the next layout.
    pub focus_text: Option<u64>,
    /// The build / transition player's timer is running.
    pub playing_timer: bool,
    /// The Find / Replace pane, while it is open.
    pub find: Option<crate::find::FindState>,
    /// The window is asking "save changes?" (the close guard).
    pub asking_close: bool,
    /// The window closes once the save in flight is written.
    pub close_after_save: bool,
}

impl AppState {
    #[must_use]
    pub fn new(args: Args, data_root: PathBuf) -> Self {
        Self {
            editor: None,
            screen: Screen::Editor,
            view: View::Normal,
            page: BackstagePage::New,
            ribbon_tab: 0,
            zoom: None,
            show_notes: true,
            guides: Vec::new(),
            marquee: None,
            media: HashMap::new(),
            decks: Vec::new(),
            new_theme: 1,
            new_variant: 0,
            new_fonts: 0,
            new_size: SlideSize::Wide,
            show: None,
            drive: Arc::new(LocalDrive::new(data_root.clone())),
            data_root,
            kit: None,
            message: String::new(),
            busy: 0,
            args,
            focus_text: None,
            playing_timer: false,
            find: None,
            asking_close: false,
            close_after_save: false,
        }
    }

    /// The canvas's px per slide unit in a `window_w` x `window_h` window:
    /// the zoom, or the slide fitted into the room the normal view leaves.
    #[must_use]
    pub fn canvas_scale(&self, window_w: f32, window_h: f32) -> f32 {
        if let Some(percent) = self.zoom {
            return (percent / 100.0).clamp(0.05, 4.0);
        }
        let (sw, sh) = self
            .editor
            .as_ref()
            .map_or((1920.0, 1080.0), |e| (e.deck.size.width(), e.deck.size.height()));
        fit_scale(window_w, window_h, sw, sh, self.show_notes)
    }

    /// The fitted zoom in percent (the status bar's slider shows it).
    #[must_use]
    pub fn zoom_percent(&self, window_w: f32, window_h: f32) -> f32 {
        (self.canvas_scale(window_w, window_h) * 100.0).round()
    }
}

/// The rail's share of the window (the shell's navigation split).
pub const NAVIGATION_RATIO: f32 = 0.17;
/// The document's share of the rest (the format pane has the remainder).
pub const DOCUMENT_RATIO: f32 = 0.78;
/// The zoom range in percent: what the zoom buttons reach and what the
/// status bar's slider spans.
pub const ZOOM_MIN: f32 = 10.0;
/// See [`ZOOM_MIN`].
pub const ZOOM_MAX: f32 = 400.0;

/// The room the normal view leaves the slide: the window minus the rail,
/// the format pane, the title row, the ribbon, the status bar, the notes
/// and a margin; the slide fitted into it.
#[must_use]
pub fn fit_scale(window_w: f32, window_h: f32, slide_w: f32, slide_h: f32, notes: bool) -> f32 {
    let room_w = window_w * (1.0 - NAVIGATION_RATIO) * DOCUMENT_RATIO - 48.0;
    let room_h = window_h - 32.0 - 128.0 - 28.0 - if notes { 120.0 } else { 0.0 } - 48.0;
    (room_w / slide_w).min(room_h / slide_h).clamp(0.05, 4.0)
}

/// Everything the user can ask for, from the ribbon, the backstage, a
/// context menu or a key.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    // ---- file ----
    OpenBackstage(BackstagePage),
    CloseBackstage,
    NewDeck,
    NewTheme(usize),
    NewVariant(usize),
    NewFonts(usize),
    NewSize(SlideSize),
    OpenDeck(String),
    OpenSample,
    RefreshDecks,
    Save,
    ExportPdf,
    ExportImages,
    CloseDeck,
    // ---- edit ----
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Duplicate,
    Delete,
    SelectAll,
    /// The Find / Replace pane (`true`: with the replace field).
    Find(bool),
    /// Leaves the text being edited (Escape).
    StopEditing,
    // ---- slides ----
    NewSlide(LayoutKind),
    Layout(LayoutKind),
    ResetSlide,
    AddSection,
    ToggleHidden,
    DuplicateSlides,
    DeleteSlides,
    // ---- text ----
    Bold,
    Italic,
    Underline,
    Strike,
    Align(Align),
    Bullets,
    Indent(i8),
    Grow(i32),
    TextColor(Option<Color>),
    Font(Option<String>),
    // ---- drawing / insert ----
    Shape(ShapeKind),
    TextBox,
    Table,
    Chart(ChartKind),
    Video,
    Picture,
    Arrange(ZOrder),
    /// The selected pictures' fit (FORMAT > Picture).
    ImageFit(ImageFit),
    Group,
    Ungroup,
    Fill(Option<Color>),
    Outline(Option<Color>),
    // ---- design ----
    Theme(usize),
    Variant(usize),
    FontScheme(usize),
    SlideSize(SlideSize),
    Background(Option<Background>, bool),
    // ---- transitions / animations ----
    Transition(TransitionKind),
    TransitionDuration(i32),
    TransitionToAll,
    Animation(Option<AnimationEffect>),
    MoveAnimation(i32),
    // ---- slide show ----
    StartShow { from_current: bool },
    ShowNext,
    ShowPrev,
    ShowEnd,
    ShowBlank(Blank),
    ShowGoto(usize),
    // ---- view ----
    View(View),
    /// Slide `n` (0-based) on the canvas, in the normal view.
    GoToSlide(usize),
    Zoom(i32),
    ZoomFit,
    ToggleNotes,
    RibbonTab(usize),
}

/// A button's payload: the app and the command it runs.
pub struct CommandData {
    pub app: RefAny,
    pub cmd: Command,
}

/// The payload of a control that runs `cmd` on `app`.
#[must_use]
pub fn command(app: &RefAny, cmd: Command) -> RefAny {
    RefAny::new(CommandData {
        app: app.clone(),
        cmd,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fitted_slide_fills_the_room_left_by_the_chrome() {
        let s = fit_scale(1280.0, 800.0, 1920.0, 1080.0, true);
        assert!(s > 0.35 && s < 0.45, "{s}");
        let without_notes = fit_scale(1280.0, 800.0, 1920.0, 1080.0, false);
        assert!(without_notes >= s);
        assert_eq!(fit_scale(10.0, 10.0, 1920.0, 1080.0, true), 0.05, "never below 5%");
    }

    #[test]
    fn the_presenter_view_opens_on_a_screen_of_its_own_or_on_the_chosen_one() {
        let auto = PresenterMonitor::Automatic;
        // The show on the primary (0): the presenter on the other screen.
        assert_eq!(auto.resolve(&[0, 1], Some(0)), Some(1));
        assert_eq!(auto.resolve(&[0, 1], Some(1)), Some(0));
        assert_eq!(auto.resolve(&[0, 1, 2], Some(1)), Some(0));
        // One screen: the system places it.
        assert_eq!(auto.resolve(&[0], Some(0)), None);
        assert_eq!(auto.resolve(&[], None), None);
        // The show's monitor unknown: the second screen, if any.
        assert_eq!(auto.resolve(&[0, 1], None), Some(1));
        // A chosen monitor wins while it is connected, even the show's own.
        let chosen = PresenterMonitor::Monitor(2);
        assert_eq!(chosen.resolve(&[0, 1, 2], Some(0)), Some(2));
        assert_eq!(PresenterMonitor::Monitor(0).resolve(&[0, 1], Some(0)), Some(0));
        // Unplugged: automatic again.
        assert_eq!(chosen.resolve(&[0, 1], Some(0)), Some(1));
        // The choice survives settings.json.
        for choice in [auto, chosen] {
            assert_eq!(PresenterMonitor::parse(Some(&choice.to_setting())), choice);
        }
        assert_eq!(PresenterMonitor::parse(None), auto);
        assert_eq!(PresenterMonitor::parse(Some("not a monitor")), auto);
    }

    #[test]
    fn the_views_and_pages_know_their_place() {
        assert_eq!(View::Outline.index(), 2);
        assert_eq!(BackstagePage::Open.index(), 2);
        assert_eq!(BackstagePage::NAV.len(), 8);
        assert_eq!(BackstagePage::About.label(), "About");
    }
}
