//! The app's state and its plumbing: the start (switches, the kit, the window), the data in the
//! data tree (`player/history.json`, `player/library.json`, through the azul-storage Drive on a
//! Thread), the library scans and the pictures on their workers (`scan.rs`), the tick (the
//! video's curtain, the music's queue, the slide show, the chrome that hides itself, the clock).
//! What plays is `media.rs`, where the focus goes and what a key does is `nav.rs`, the window is
//! `ui.rs`.
//!
//! WHAT CHANGES OFTEN IS CHANGED IN PLACE: the chrome over a video or a slide show and the OSD
//! (their `opacity`, which their declared fade then eases), the time played and the seek bar,
//! the clock. A rebuild is left for what the user does and for a new phase of what plays - and a
//! rebuild is where the motion comes from: the engine slides what moved, fades in what came and
//! fades out what went.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
};

use azul::{
    audio::AudioPlayer,
    callbacks::{TimerCallbackInfo, TimerCallbackReturn, WriteBackCallbackType},
    css::{FloatValue, PercentageValue, StyleOpacity},
    file::FilePath,
    image::ImageRef,
    option::OptionFilePath,
    prelude::*,
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
};
use azul_appkit::{
    about::AboutInfo,
    files::{FileJob, FileOutcome},
    shortcuts::Shortcut,
    ui as kit,
};

use crate::{
    args::{Args, FolderArgs, SPEC},
    gallery::Grid,
    history::{History, HISTORY_FILE},
    ids,
    library::{Item, Library, Shelf, Status, LIBRARY_FILE, RECORDED_TV},
    media::{self, Music, VideoSession, Viewer},
    pages::{self, Place, Screen, Section, Tile},
    scan::{self, ArtDone, ArtJob, ArtSource, ScanBatch},
    strip::StripFocus,
    sync::ControlsVisibility,
};

// ==== The app's facts ====

pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzPlayer",
    version: env!("CARGO_PKG_VERSION"),
    summary: "A media center in the look of Windows Media Center: music, pictures and slide \
              shows, videos and movies. Videos play with azul's VideoWidget, their sound with \
              azul's AudioPlayer, started together; recent files resume where you stopped. Part \
              of the Azlin apps, built with azul.",
    license: "MIT",
    app_folder: "player",
};

/// The keyboard shortcuts the settings page lists.
pub const SHORTCUTS: [Shortcut; 14] = [
    Shortcut::new("Moving around", "Arrow keys", "Move the focus"),
    Shortcut::new("Moving around", "Enter", "Open, play"),
    Shortcut::new("Moving around", "Backspace  Escape", "Back"),
    Shortcut::new("Moving around", "Home", "The start screen"),
    Shortcut::new("Moving around", "Mouse wheel", "Scroll the strip or the gallery"),
    Shortcut::new("Playback", "Space", "Play / pause"),
    Shortcut::new("Playback", "Left  Right", "Back / forward 10 seconds (videos, music)"),
    Shortcut::new("Playback", "Shift+Left  Shift+Right", "Back / forward a minute"),
    Shortcut::new("Playback", "Page Up  Page Down", "Previous / next song or picture"),
    Shortcut::new("Playback", "Up  Down", "Volume up / down (while something plays)"),
    Shortcut::new("Playback", "M", "Mute"),
    Shortcut::new("Window", "F  F11  Double-click", "Fullscreen (media only)"),
    Shortcut::new("Window", "Escape", "Leave fullscreen"),
    Shortcut::new("File", "Mod+O", "Open a video"),
];

/// The settings page's own categories (none: the kit's are enough).
const APP_CATEGORIES: [&str; 0] = [];

/// How often the tick looks at what plays, the curtain and the chrome.
pub const TICK_MS: u64 = 100;
/// How long the OSD and a notice show.
pub const OSD_MS: u64 = 1_200;
pub const NOTICE_MS: u64 = 3_200;

/// The write-back tags of the app's file jobs.
const TAG_LOAD: u64 = 1;
const TAG_SAVE: u64 = 2;

// ==== State ====

/// The folders the libraries read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Folders {
    pub music: PathBuf,
    pub pictures: PathBuf,
    pub videos: PathBuf,
    pub tv: PathBuf,
}

impl Folders {
    /// The folders asked for on the command line, else the user's own (`FilePath`'s, else the
    /// home folder's `Music`, `Pictures`, `Videos`); recorded TV is in the videos folder.
    #[must_use]
    pub fn resolve(asked: &FolderArgs) -> Folders {
        let path = |p: OptionFilePath| -> Option<PathBuf> {
            p.into_option()
                .map(|dir| PathBuf::from(dir.inner.as_str()))
                .filter(|p| !p.as_os_str().is_empty())
        };
        let home = path(FilePath::get_home_dir()).unwrap_or_default();
        let music = asked
            .music
            .clone()
            .or_else(|| path(FilePath::get_audio_dir()))
            .unwrap_or_else(|| home.join("Music"));
        let pictures = asked
            .pictures
            .clone()
            .or_else(|| path(FilePath::get_picture_dir()))
            .unwrap_or_else(|| home.join("Pictures"));
        let videos = asked
            .videos
            .clone()
            .or_else(|| path(FilePath::get_video_dir()))
            .unwrap_or_else(|| home.join("Videos"));
        let tv = asked.tv.clone().unwrap_or_else(|| videos.join(RECORDED_TV));
        Folders {
            music,
            pictures,
            videos,
            tv,
        }
    }

    /// The folder of `shelf`.
    #[must_use]
    pub fn of(&self, shelf: Shelf) -> &PathBuf {
        match shelf {
            Shelf::Music => &self.music,
            Shelf::Pictures => &self.pictures,
            Shelf::Videos => &self.videos,
            Shelf::Tv => &self.tv,
        }
    }
}

/// A picture made on a worker: the image and its size in pixels (`None`: it could not be read).
pub type Art = Option<(ImageRef, f32, f32)>;

/// The app's state.
pub struct Player {
    /// The appkit kit (settings, data root, the settings page's state).
    pub kit: RefAny,
    pub args: Args,
    pub folders: Folders,
    pub history: History,
    pub library: Library,
    /// What a running scan found so far, per library (in [`Shelf::ALL`] order): swapped in when
    /// it is done (or shown at once while the library is empty).
    pub scanning: [Option<Vec<Item>>; 4],
    /// The pictures made so far (thumbnails, covers, the viewer's copies), by key.
    pub art: HashMap<String, Art>,
    /// The keys a worker is making.
    pub art_pending: HashSet<String>,
    /// The pictures' keys in the order they were made: the oldest go first when the cache is
    /// full ([`ART_KEPT`], [`FULL_KEPT`]).
    pub art_order: VecDeque<String>,
    /// A picture worker runs (one at a time).
    pub art_busy: bool,
    /// The pages, the first the start strip; the last shows.
    pub nav: Vec<Place>,
    pub strip: StripFocus,
    /// The search's words.
    pub query: String,
    /// The address page's field (a video on a web server).
    pub address: String,
    pub viewer: Viewer,
    /// The sound: music, and a video's sound.
    pub audio: Option<AudioPlayer>,
    pub music: Option<Music>,
    pub video: Option<VideoSession>,
    pub volume: f32,
    pub muted: bool,
    pub fullscreen: bool,
    /// The pointer or a key last moved (the chrome and the corner's back button show).
    pub controls: ControlsVisibility,
    /// The chrome showed at the last look (shown and hidden in place).
    pub controls_shown: bool,
    /// The on-screen display and until when it shows (ms on `clock`).
    pub osd: Option<(String, u64)>,
    /// A sentence for a moment (why an item cannot work here, a library that is empty).
    pub notice: Option<(String, u64)>,
    /// The app's monotonic clock.
    pub clock: std::time::Instant,
    pub ticking: bool,
    /// The window's size (logical px).
    pub window: (f32, f32),
    /// The wheel's travel not yet turned into a step.
    pub wheel: f32,
    /// The clock's text as shown ("21:45").
    pub clock_text: String,
}

impl Player {
    fn new(kit: RefAny, args: Args) -> Self {
        let folders = Folders::resolve(&args.folders);
        let window = args.kit.size.unwrap_or((1100.0, 700.0));
        let mut nav = vec![Place::new(Screen::Start)];
        let screen = args.kit.screen.clone().unwrap_or_default();
        if let Some(section) = Section::by_screen_name(&screen) {
            nav.push(Place::new(Screen::Section(section)));
        } else if screen == "now-playing" {
            nav.push(Place::new(Screen::NowPlaying));
        }
        Self {
            kit,
            args,
            folders,
            history: History::default(),
            library: Library::default(),
            scanning: [None, None, None, None],
            art: HashMap::new(),
            art_pending: HashSet::new(),
            art_order: VecDeque::new(),
            art_busy: false,
            nav,
            strip: StripFocus::default(),
            query: String::new(),
            address: String::new(),
            viewer: Viewer::default(),
            audio: None,
            music: None,
            video: None,
            volume: 1.0,
            muted: false,
            fullscreen: false,
            controls: ControlsVisibility::default(),
            controls_shown: true,
            osd: None,
            notice: None,
            clock: std::time::Instant::now(),
            ticking: false,
            window,
            wheel: 0.0,
            clock_text: wall_clock(),
        }
    }

    /// Milliseconds on the app's clock.
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        u64::try_from(self.clock.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// The page that shows.
    #[must_use]
    pub fn place(&self) -> &Place {
        // `nav` always holds the start strip.
        &self.nav[self.nav.len() - 1]
    }

    pub fn place_mut(&mut self) -> &mut Place {
        let last = self.nav.len() - 1;
        &mut self.nav[last]
    }

    /// The page the MENUS show: the page under a video while its curtain is down.
    #[must_use]
    pub fn menus_place(&self) -> &Place {
        let n = self.nav.len();
        if self.place().screen == Screen::Video && n >= 2 {
            &self.nav[n - 2]
        } else {
            self.place()
        }
    }

    /// The audio player, made on first use.
    pub fn player(&mut self) -> &AudioPlayer {
        let volume = if self.muted { 0.0 } else { self.volume };
        self.audio.get_or_insert_with(|| {
            let p = AudioPlayer::create();
            p.set_volume(volume);
            p
        })
    }

    /// The data root and the app's key for `name`.
    fn root_and_key(&self, name: &str) -> Option<(PathBuf, String)> {
        let mut kit = self.kit.clone();
        let found = kit
            .downcast_ref::<kit::Kit>()
            .map(|k| (k.data_root.clone(), k.key(name)));
        found
    }

    /// Shows `text` on the OSD for a moment.
    pub fn osd(&mut self, text: String) {
        let until = self.now_ms() + OSD_MS;
        self.osd = Some((text, until));
    }

    /// Shows a sentence for a moment (why an item cannot work, a library that is empty).
    pub fn notice(&mut self, text: &str) {
        let until = self.now_ms() + NOTICE_MS;
        println!("AZPLAYER_NOTICE {text}");
        self.notice = Some((text.to_string(), until));
    }

    /// The pointer or a key moved: the chrome shows (and hides itself later).
    pub fn activity(&mut self) {
        let now = self.now_ms();
        self.controls.activity(now);
    }

    /// The items of `shelf` as shown (a running scan's, while the library is still empty).
    #[must_use]
    pub fn items(&self, shelf: Shelf) -> &[Item] {
        &self.library.shelf(shelf).items
    }

    /// The tiles of the page that shows (a library's page, a group's page, search).
    #[must_use]
    pub fn page_tiles(&self, place: &Place) -> Vec<Tile> {
        match &place.screen {
            Screen::Section(section) => {
                let views = section.views();
                let view = views[place.focus.view.min(views.len() - 1)];
                pages::tiles(*section, view, &self.library, &self.history)
            }
            Screen::Group { shelf, group, .. } => pages::group_tiles(*shelf, group),
            Screen::Search => search_tiles(&self.library, &self.query),
            _ => Vec::new(),
        }
    }

    /// The gallery's grid on a page with `tiles`, in this window.
    #[must_use]
    pub fn grid(&self, tiles: &[Tile]) -> Grid {
        let (w, h) = self.gallery_area();
        Grid::new(pages::tile_kind(tiles), w, h)
    }

    /// The gallery's area (logical px): under the title and the views, over the status line.
    #[must_use]
    pub fn gallery_area(&self) -> (f32, f32) {
        let title_row = if self.fullscreen { 0.0 } else { 32.0 };
        (
            (self.window.0 - crate::ui::GALLERY_LEFT).max(100.0),
            (self.window.1 - title_row - crate::ui::GALLERY_TOP - crate::ui::GALLERY_BOTTOM)
                .max(80.0),
        )
    }
}

/// The search's tiles: every song, picture and video whose words match.
#[must_use]
pub fn search_tiles(library: &Library, query: &str) -> Vec<Tile> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for shelf in [Shelf::Music, Shelf::Videos, Shelf::Tv, Shelf::Pictures] {
        let items = &library.shelf(shelf).items;
        for index in crate::library::by_title(items) {
            if crate::library::matches(&items[index], query) {
                out.push(Tile::Item { shelf, index });
            }
        }
    }
    out
}

/// The time of day, "21:45" (UTC when the local offset is not known to the standard library:
/// the clock asks the system for it through `date`'s format only where it can).
#[must_use]
pub fn wall_clock() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let local = secs.saturating_add_signed(local_offset_s());
    format!("{}:{:02}", (local / 3600) % 24, (local / 60) % 60)
}

/// The local time's offset from UTC in seconds, read once from the system (`date +%z`; 0 when
/// it cannot be read).
fn local_offset_s() -> i64 {
    static OFFSET: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *OFFSET.get_or_init(|| {
        let Ok(out) = std::process::Command::new("date").arg("+%z").output() else {
            return 0;
        };
        let text = String::from_utf8_lossy(&out.stdout);
        let t = text.trim();
        if t.len() != 5 {
            return 0;
        }
        let sign = if t.starts_with('-') { -1 } else { 1 };
        let hours: i64 = t[1..3].parse().unwrap_or(0);
        let minutes: i64 = t[3..5].parse().unwrap_or(0);
        sign * (hours * 3600 + minutes * 60)
    })
}

/// The app's start: switches, the kit (settings, data root), the window.
pub fn start() {
    let args = match Args::from_env() {
        Ok(a) => a,
        Err(message) => {
            println!("{message}");
            std::process::exit(if message.contains("USAGE") { 0 } else { 2 });
        }
    };
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &APP_CATEGORIES, args.kit.clone());
    if args.kit.screen.as_deref() == Some("settings") {
        kit::open_settings(&kit_ref, None);
    }
    let app = Player::new(kit_ref.clone(), args);
    let mut config = kit::app_config(&kit_ref);
    config.expose_system_media_controls = true;
    // The focus is the app's own Media Center glow, drawn on every focusable part (`look.rs`):
    // the engine's blue ring around the focused box is off.
    config.system_animations.focus_ring_duration_ms = 0;
    // The renderer is the desktop default (the CPU renderer, what the E2E runs see too): the
    // pages' exits - a page fading out as the next comes in, the slide show's cross-fade - are
    // drawn there (`AZ_BACKEND=gpu` still picks WebRender, where an exit is not drawn yet).
    let window = kit::window_options(
        &kit_ref,
        crate::ui::layout,
        (1100.0, 700.0),
        (640.0, 420.0),
        on_window_created,
    );
    App::create(RefAny::new(app), config).run(window);
}

// ==== The data tree: the history and the library file ====

extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some((kit_ref, history, library)) = data.downcast_ref::<Player>().map(|s| {
        (
            s.kit.clone(),
            s.root_and_key(HISTORY_FILE),
            s.root_and_key(LIBRARY_FILE),
        )
    }) else {
        return Update::DoNothing;
    };
    kit::on_window_created(&kit_ref, &mut info);
    if let (Some((root, history)), Some((_, library))) = (history, library) {
        kit::spawn_file_jobs(
            &mut info,
            &root,
            vec![FileJob::Get { key: history }, FileJob::Get { key: library }],
            app.clone(),
            TAG_LOAD,
            on_files as WriteBackCallbackType,
        );
    }
    ensure_ticking(&app, &mut info);
    Update::DoNothing
}

extern "C" fn on_files(mut data: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let app = data.clone();
    let first = {
        let Some(mut s) = data.downcast_mut::<Player>() else {
            return Update::DoNothing;
        };
        if reply.tag == TAG_SAVE {
            for outcome in reply.outcomes {
                if let Some(why) = outcome.error() {
                    println!("AZPLAYER_ERROR {why}");
                }
            }
            return Update::DoNothing;
        }
        for outcome in reply.outcomes {
            if let FileOutcome::Got {
                key,
                result: Ok(Some(bytes)),
            } = outcome
            {
                let text = String::from_utf8_lossy(&bytes);
                if key.ends_with(HISTORY_FILE) {
                    if let Ok(h) = History::from_json(&text) {
                        s.history = h;
                    }
                } else if key.ends_with(LIBRARY_FILE) {
                    match Library::from_json(&text) {
                        Ok(library) => s.library = library,
                        Err(why) => println!("AZPLAYER_ERROR {why}"),
                    }
                }
            }
        }
        println!("AZPLAYER_HISTORY {}", s.history.entries.len());
        // The page it opens on, announced like every page it goes to after.
        println!("AZPLAYER_PAGE {}", s.place().screen.key());
        println!(
            "AZPLAYER_LIBRARY cached {} {} {} {}",
            s.library.music.items.len(),
            s.library.pictures.items.len(),
            s.library.videos.items.len(),
            s.library.tv.items.len()
        );
        s.args
            .kit
            .files
            .first()
            .map(|p| p.to_string_lossy().into_owned())
    };
    // The libraries are scanned again, every start, on their workers.
    scan_all(&app, &mut info);
    if let Some(path) = first {
        media::open_video(&app, &mut info, &path);
    }
    request_art(&app, &mut info);
    Update::RefreshDom
}

/// Writes the history into the data tree, on a Thread.
pub fn save_history(app: &RefAny, s: &Player, info: &mut CallbackInfo) {
    if let Some((root, key)) = s.root_and_key(HISTORY_FILE) {
        kit::spawn_file_jobs(
            info,
            &root,
            vec![FileJob::Put {
                key,
                bytes: s.history.to_json().into_bytes(),
            }],
            app.clone(),
            TAG_SAVE,
            on_files as WriteBackCallbackType,
        );
    }
}

/// Writes the library file into the data tree, on a Thread.
fn save_library(app: &RefAny, s: &Player, info: &mut CallbackInfo) {
    if let Some((root, key)) = s.root_and_key(LIBRARY_FILE) {
        kit::spawn_file_jobs(
            info,
            &root,
            vec![FileJob::Put {
                key,
                bytes: s.library.to_json().into_bytes(),
            }],
            app.clone(),
            TAG_SAVE,
            on_files as WriteBackCallbackType,
        );
    }
}

// ==== The library scans ====

/// Scans every library again, each on its own worker.
pub fn scan_all(app: &RefAny, info: &mut CallbackInfo) {
    let mut app_ref = app.clone();
    let Some(mut s) = app_ref.downcast_mut::<Player>() else {
        return;
    };
    for (i, shelf) in Shelf::ALL.into_iter().enumerate() {
        if s.scanning[i].is_some() {
            continue;
        }
        let folder = s.folders.of(shelf).clone();
        // Recorded TV is its own library: the video scan leaves its folder out.
        let skip = (shelf == Shelf::Videos).then(|| s.folders.tv.clone());
        s.scanning[i] = Some(Vec::new());
        s.library.shelf_mut(shelf).status = Status::Scanning;
        s.library.shelf_mut(shelf).folder = folder.to_string_lossy().into_owned();
        scan::spawn_scan(info, app, shelf, folder, skip, on_scan as WriteBackCallbackType);
    }
}

extern "C" fn on_scan(mut data: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(batch) = msg.downcast_mut::<ScanBatch>().map(|mut b| ScanBatch {
        shelf: b.shelf,
        items: std::mem::take(&mut b.items),
        first: b.first,
        done: b.done,
        cut: b.cut,
        missing: b.missing,
    }) else {
        return Update::DoNothing;
    };
    let app = data.clone();
    let (refresh, save) = {
        let Some(mut s) = data.downcast_mut::<Player>() else {
            return Update::DoNothing;
        };
        let i = Shelf::ALL
            .iter()
            .position(|x| *x == batch.shelf)
            .unwrap_or(0);
        let found = s.scanning[i].get_or_insert_with(Vec::new);
        if batch.first {
            found.clear();
        }
        found.extend(batch.items);
        let empty = s.library.shelf(batch.shelf).items.is_empty();
        if batch.done {
            let items = s.scanning[i].take().unwrap_or_default();
            let shelf = s.library.shelf_mut(batch.shelf);
            let changed = shelf.items != items;
            shelf.items = items;
            shelf.cut = batch.cut;
            shelf.status = if batch.missing {
                Status::Missing
            } else {
                Status::Ready
            };
            println!(
                "AZPLAYER_SCAN {} {} {}",
                batch.shelf.word().replace(' ', "-"),
                shelf.items.len(),
                if batch.missing { "missing" } else { "ready" }
            );
            (true, changed)
        } else if empty {
            // A first scan shows what it finds as it goes.
            let so_far = s.scanning[i].clone().unwrap_or_default();
            s.library.shelf_mut(batch.shelf).items = so_far;
            (true, false)
        } else {
            (false, false)
        }
    };
    if save {
        let mut app_ref = app.clone();
        if let Some(s) = app_ref.downcast_ref::<Player>() {
            save_library(&app, &s, &mut info);
        };
    }
    if refresh {
        clamp_focus(&app);
        request_art(&app, &mut info);
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// A library that changed under a page: its focus stays on a tile that exists.
fn clamp_focus(app: &RefAny) {
    let mut app_ref = app.clone();
    let Some(mut s) = app_ref.downcast_mut::<Player>() else {
        return;
    };
    let place = s.place().clone();
    let count = s.page_tiles(&place).len();
    let focus = &mut s.place_mut().focus;
    if count == 0 {
        focus.index = 0;
        focus.first_col = 0;
    } else if focus.index >= count {
        focus.index = count - 1;
    }
}

// ==== The pictures ====

/// The key of a picture file's thumbnail, of a song's cover, of the viewer's copy.
#[must_use]
pub fn thumb_key(path: &str) -> String {
    format!("t:{path}")
}
#[must_use]
pub fn cover_key(path: &str) -> String {
    format!("c:{path}")
}
#[must_use]
pub fn full_key(path: &str) -> String {
    format!("f:{path}")
}

/// The thumbnails' size on the worker (device px: twice the tiles' logical size).
const THUMB_PX: u32 = 320;
/// The covers' size (now playing shows them large).
const COVER_PX: u32 = 400;
/// How many thumbnails and covers are kept (a 320 px thumbnail is about 300 KB), and how many
/// window-sized copies (one is 20 MB at 2560 px).
pub const ART_KEPT: usize = 160;
pub const FULL_KEPT: usize = 3;

/// The picture a tile shows, as a job: a picture's thumbnail, an album's cover (its first song
/// with one), a picture folder's first picture.
#[must_use]
pub fn tile_art(s: &Player, tile: &Tile) -> Option<ArtJob> {
    let job = |key: String, source: ArtSource, max_px: u32| ArtJob {
        key,
        source,
        max_px,
    };
    match tile {
        Tile::Item {
            shelf: Shelf::Pictures,
            index,
        } => {
            let path = &s.items(Shelf::Pictures).get(*index)?.path;
            Some(job(thumb_key(path), ArtSource::Picture(path.clone()), THUMB_PX))
        }
        Tile::Group {
            shelf: Shelf::Pictures,
            group,
        } => {
            let path = &s.items(Shelf::Pictures).get(*group.items.first()?)?.path;
            Some(job(thumb_key(path), ArtSource::Picture(path.clone()), THUMB_PX))
        }
        Tile::Group {
            shelf: Shelf::Music,
            group,
        } => {
            let songs = s.items(Shelf::Music);
            let path = &group
                .items
                .iter()
                .filter_map(|i| songs.get(*i))
                .find(|song| song.has_cover)?
                .path;
            Some(job(cover_key(path), ArtSource::Cover(path.clone()), COVER_PX))
        }
        _ => None,
    }
}

/// The viewer's copy of a picture: the window's size, on a retina screen twice that.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn full_job(s: &Player, path: &str) -> ArtJob {
    let side = (s.window.0.max(s.window.1) * 2.0).clamp(800.0, 2560.0) as u32;
    ArtJob {
        key: full_key(path),
        source: ArtSource::Picture(path.to_string()),
        max_px: side,
    }
}

/// The pictures the window wants and nobody has made yet: the tiles in view, the song that
/// plays, the viewer's picture and the next one.
fn wanted_art(s: &Player) -> Vec<ArtJob> {
    let mut jobs: Vec<ArtJob> = Vec::new();
    let place = s.menus_place().clone();
    let tiles = s.page_tiles(&place);
    if !tiles.is_empty() {
        let grid = s.grid(&tiles);
        for i in grid.built(place.focus.first_col, tiles.len()) {
            if let Some(job) = tile_art(s, &tiles[i]) {
                jobs.push(job);
            }
        }
    }
    if place.screen == Screen::Start || place.screen == Screen::NowPlaying || s.music.is_some() {
        if let Some(song) = s.music.as_ref().and_then(|m| m.current_item(s)) {
            if song.has_cover {
                jobs.push(ArtJob {
                    key: cover_key(&song.path),
                    source: ArtSource::Cover(song.path.clone()),
                    max_px: COVER_PX,
                });
            }
        }
    }
    if place.screen == Screen::Picture {
        let n = s.viewer.paths.len();
        if n > 0 {
            for k in [s.viewer.index, (s.viewer.index + 1) % n] {
                jobs.push(full_job(s, &s.viewer.paths[k]));
            }
        }
    }
    let mut seen = HashSet::new();
    jobs.retain(|j| {
        !s.art.contains_key(&j.key) && !s.art_pending.contains(&j.key) && seen.insert(j.key.clone())
    });
    jobs.truncate(24);
    jobs
}

/// Starts a picture worker for what the window wants, unless one runs (it asks again when it is
/// done).
pub fn request_art(app: &RefAny, info: &mut CallbackInfo) {
    let mut app_ref = app.clone();
    let Some(mut s) = app_ref.downcast_mut::<Player>() else {
        return;
    };
    if s.art_busy {
        return;
    }
    let jobs = wanted_art(&s);
    if jobs.is_empty() {
        return;
    }
    for job in &jobs {
        s.art_pending.insert(job.key.clone());
    }
    s.art_busy = true;
    scan::spawn_art(info, app, jobs, on_art as WriteBackCallbackType);
}

extern "C" fn on_art(mut data: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(done) = msg.downcast_mut::<ArtDone>().map(|mut d| ArtDone {
        key: std::mem::take(&mut d.key),
        image: d.image.take(),
        last: d.last,
    }) else {
        return Update::DoNothing;
    };
    let app = data.clone();
    {
        let Some(mut s) = data.downcast_mut::<Player>() else {
            return Update::DoNothing;
        };
        s.art_pending.remove(&done.key);
        s.art_order.retain(|k| *k != done.key);
        s.art_order.push_back(done.key.clone());
        s.art.insert(done.key, done.image);
        // The oldest go when the cache is full: window-sized copies beyond the last few, the
        // rest beyond their count (a page brought back asks for them again).
        let fulls = s.art_order.iter().filter(|k| k.starts_with("f:")).count();
        let mut drop_full = fulls.saturating_sub(FULL_KEPT);
        let mut drop_rest = (s.art_order.len() - fulls).saturating_sub(ART_KEPT);
        let mut kept = VecDeque::with_capacity(s.art_order.len());
        while let Some(k) = s.art_order.pop_front() {
            let full = k.starts_with("f:");
            if full && drop_full > 0 {
                drop_full -= 1;
                s.art.remove(&k);
            } else if !full && drop_rest > 0 {
                drop_rest -= 1;
                s.art.remove(&k);
            } else {
                kept.push_back(k);
            }
        }
        s.art_order = kept;
        if done.last {
            s.art_busy = false;
            s.art_pending.clear();
        }
    }
    if done.last {
        request_art(&app, &mut info);
    }
    Update::RefreshDom
}

// ==== In place: the chrome, the OSD, the labels ====

/// Shows (`true`) or hides the node marked `marker` in place: its `opacity` (the node's declared
/// fade eases it), no rebuild, no relayout. Nothing when it is not in the window.
pub fn set_shown(info: &mut CallbackInfo, marker: AzString, shown: bool) {
    if let Some(node) = info.get_node_id_by_marker(marker).into_option() {
        // An opacity is a percentage: 100 is opaque.
        let percent = if shown { 100.0 } else { 0.0 };
        let opacity = StyleOpacity {
            inner: PercentageValue {
                number: FloatValue::create(percent),
            },
        };
        info.set_css_property(node, CssProperty::opacity(opacity));
    }
}

/// Rewrites the text node marked `marker` in place.
pub fn set_text(info: &mut CallbackInfo, marker: AzString, text: &str) {
    if let Some(node) = info.get_node_id_by_marker(marker).into_option() {
        info.change_node_text(node, AzString::from(text));
    }
}

/// The chrome over what plays (the top and the bottom strips) and the corner's back button, on
/// or off, in place.
pub fn show_chrome(info: &mut CallbackInfo, shown: bool) {
    set_shown(info, ids::TOP, shown);
    set_shown(info, ids::BAR, shown);
    set_shown(info, ids::CORNER, shown);
}

/// The OSD as the state has it, in place: its text and whether it shows.
pub fn show_osd(s: &Player, info: &mut CallbackInfo) {
    match &s.osd {
        Some((text, _)) => {
            set_text(info, ids::OSD_TEXT, text);
            set_shown(info, ids::OSD, true);
        }
        None => set_shown(info, ids::OSD, false),
    }
}

// ==== The tick ====

pub fn ensure_ticking(app: &RefAny, info: &mut CallbackInfo) {
    let mut app_ref = app.clone();
    let Some(mut s) = app_ref.downcast_mut::<Player>() else {
        return;
    };
    if s.ticking {
        return;
    }
    s.ticking = true;
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(app.clone(), on_tick, get_time)
            .with_interval(Duration::System(SystemTimeDiff::from_millis(TICK_MS))),
    );
}

/// Every 100 ms: the video's curtain (is the picture ready, the sound; the next step of the
/// fade), the music's queue, the slide show, the chrome that hides itself and the OSD (in
/// place), the clock (in place, once a minute). A rebuild only when a phase changed.
extern "C" fn on_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let app = data.clone();
    let mut refresh = false;
    let mut wants_art = false;
    {
        let Some(mut guard) = data.downcast_mut::<Player>() else {
            return TimerCallbackReturn::terminate_unchanged();
        };
        let s = &mut *guard;
        let now = s.now_ms();
        let cb = &mut info.callback_info;
        refresh |= media::tick_video(s, cb, now);
        refresh |= media::tick_music(s, cb);
        let (slide, art) = media::tick_viewer(s, now);
        refresh |= slide;
        wants_art |= art;
        // The chrome over what plays hides itself while it plays, in place.
        let visible = s.controls.visible(now, media::chrome_hides(s));
        if visible != s.controls_shown {
            s.controls_shown = visible;
            show_chrome(cb, visible);
        }
        if s.osd.as_ref().is_some_and(|(_, until)| now >= *until) {
            s.osd = None;
            set_shown(cb, ids::OSD, false);
        }
        if s.notice.as_ref().is_some_and(|(_, until)| now >= *until) {
            s.notice = None;
            refresh = true;
        }
        let clock = wall_clock();
        if clock != s.clock_text {
            s.clock_text = clock;
            set_text(cb, ids::CLOCK_TEXT, &s.clock_text);
        }
    }
    if wants_art {
        request_art(&app, &mut info.callback_info);
    }
    if refresh {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
    }
}
