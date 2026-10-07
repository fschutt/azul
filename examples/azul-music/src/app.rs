//! The app's state, its start, the data (library and playlists through the Drive), the scan of
//! the music folder, playback (the queue and azul's `AudioPlayer`) and the navigation between the
//! pages. The window is `ui.rs`; the pages themselves are `page.rs`.

use std::path::PathBuf;

use azul::{
    audio::{AudioFileDecoder, AudioPlayer, AudioPlayerState, MediaPlaybackState, NowPlayingInfo},
    callbacks::{TimerCallbackInfo, TimerCallbackReturn, WriteBackCallbackType},
    dom::{DomId, DomNodeId, FocusTarget, NodeId, VirtualKeyCode},
    misc::MediaControlKind,
    option::OptionF32,
    prelude::*,
    str::String as AzString,
    task::{
        Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg,
        Timer, TimerId,
    },
    time::{Duration, SystemTimeDiff},
    widgets::{LevelMeter, LevelMeterThrottle, MediaControlsAction, SeekBar, SeekBarState},
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec, ModePref},
    files::{FileJob, FileOutcome},
    shortcuts::Shortcut,
    ui as kit,
};

use crate::{
    ids,
    library::{Library, Track, LIBRARY_FILE},
    page::{self, Catalog, Hover, Page, PageInput, SongSort, View},
    playlists::{Playlist, PLAYLISTS_DIR},
    queue::{PlayQueue, Previous, Repeat},
    sample, scan,
};

// ==== The app's facts ====

/// The screens `--screen` opens (the first is the default).
pub const SCREENS: [&str; 6] = ["recent", "songs", "albums", "artists", "genres", "settings"];

pub const SPEC: AppSpec = AppSpec {
    name: "AzMusic",
    binary: "AzMusic",
    summary: "a music player: the music folder's library, albums, artists, playlists, gapless \
              playback",
    screens: &SCREENS,
    files_help: "",
};

pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzMusic",
    version: env!("CARGO_PKG_VERSION"),
    summary: "Your music folder as a library of albums, artists and songs, played gaplessly by \
              azul's AudioPlayer, with the OS media keys. Part of the Azlin apps, built with azul.",
    license: "MIT",
    app_folder: "music",
};

/// The keyboard shortcuts the settings page lists.
pub const SHORTCUTS: [Shortcut; 9] = [
    Shortcut::new("Playback", "Space", "Play / pause"),
    Shortcut::new("Playback", "Mod+Right  Mod+Left", "Next / previous track"),
    Shortcut::new("Playback", "Mod+Up  Mod+Down", "Volume up / down"),
    Shortcut::new(
        "Playback",
        "Media keys",
        "Play, pause, next, previous, stop",
    ),
    Shortcut::new("Library", "Enter  Double-click", "Play the song from here"),
    Shortcut::new(
        "Library",
        "Right-click",
        "Play next, add to the queue or to a playlist",
    ),
    Shortcut::new("Library", "Mod+F", "Search"),
    Shortcut::new("Library", "Mod+[  Mod+]  Escape", "Back / forward"),
    Shortcut::new("Library", "Mod+R", "Scan the music folder again"),
];

/// The settings page's own category.
pub const APP_CATEGORIES: [&str; 1] = ["Library"];

/// The settings key of the music folder.
pub const FOLDER_SETTING: &str = "music_folder";

/// How often the now-playing bar follows the player.
const TICK_MS: u64 = 250;

/// The write-back tags of the app's file jobs.
const TAG_LOAD: u64 = 1;
const TAG_SAVE: u64 = 2;

// ==== State ====

/// The app's state.
pub struct Music {
    /// The appkit kit (settings, data root, the settings page's state).
    pub kit: RefAny,
    /// The library (one copy, shared by reference: a page reads it in place).
    pub library: RefAny,
    /// The library's albums, artists and genres (made when the library changes).
    pub catalog: Catalog,
    /// The playlists, by name.
    pub playlists: Vec<Playlist>,
    pub view: View,
    /// The pages Back and Forward go to.
    pub back: Vec<View>,
    pub forward: Vec<View>,
    /// The search field's text.
    pub query: String,
    /// The songs page's order.
    pub sort: SongSort,
    /// The selected song (a row of the page's songs).
    pub selected: Option<usize>,
    /// What the pointer is over in the page.
    pub hover: Hover,
    /// The window is dark (the page's VirtualView reads it).
    pub dark: bool,
    pub queue: PlayQueue,
    /// The player, made with the first play.
    pub player: Option<AudioPlayer>,
    /// The player's track ids and the library's track ids they play.
    pub playing: Vec<(u64, String)>,
    /// The player's id of the track handed over for gapless playback (the queue's next).
    pub queued_next: Option<u64>,
    /// What the player said last.
    pub state: AudioPlayerState,
    /// The volume the user set (0..=1), and the one before a mute.
    pub volume: f32,
    pub unmuted: f32,
    /// The meter's throttle.
    pub meter: LevelMeterThrottle,
    /// What the time-played and the length labels show (rewritten in place when they change).
    pub elapsed: String,
    pub total: String,
    /// The playback timer runs.
    pub ticking: bool,
    /// The library file was read (or found missing).
    pub loaded: bool,
    pub scanning: bool,
    /// The status line.
    pub status: String,
    /// The window's size.
    pub window: (f32, f32),
    pub args: AppArgs,
}

/// The library, as the pages see it.
pub struct LibraryRef {
    pub library: Library,
}

impl Music {
    fn new(kit: RefAny, args: AppArgs) -> Self {
        let view = match args.screen.as_deref() {
            Some("songs") => View::Songs,
            Some("albums") => View::Albums,
            Some("artists") => View::Artists,
            Some("genres") => View::Genres,
            _ => View::RecentlyAdded,
        };
        Self {
            kit,
            library: RefAny::new(LibraryRef {
                library: Library::default(),
            }),
            catalog: Catalog::default(),
            playlists: Vec::new(),
            view,
            back: Vec::new(),
            forward: Vec::new(),
            query: String::new(),
            sort: SongSort::default(),
            selected: None,
            hover: Hover::None,
            dark: true,
            queue: PlayQueue::default(),
            player: None,
            playing: Vec::new(),
            queued_next: None,
            state: AudioPlayerState::default(),
            volume: 1.0,
            unmuted: 1.0,
            meter: LevelMeterThrottle::create(100),
            elapsed: String::new(),
            total: String::new(),
            ticking: false,
            loaded: false,
            scanning: false,
            status: String::new(),
            window: args.size.unwrap_or((1200.0, 760.0)),
            args,
        }
    }

    /// A copy of the library (small: tags only).
    #[must_use]
    pub fn library(&self) -> Library {
        self.with_library(Library::clone)
    }

    /// `f` of the library, read in place.
    pub fn with_library<R>(&self, f: impl FnOnce(&Library) -> R) -> R {
        let mut r = self.library.clone();
        let out = match r.downcast_ref::<LibraryRef>() {
            Some(l) => f(&l.library),
            None => f(&Library::default()),
        };
        out
    }

    fn set_library(&mut self, library: Library) {
        self.catalog = Catalog::of(&library);
        let mut r = self.library.clone();
        let guard = r.downcast_mut::<LibraryRef>();
        if let Some(mut l) = guard {
            l.library = library;
        }
        self.selected = None;
        self.hover = Hover::None;
    }

    /// The data root and the app's key for `name`.
    fn root_and_key(&self, name: &str) -> Option<(PathBuf, String)> {
        let mut kit = self.kit.clone();
        let found = kit
            .downcast_ref::<kit::Kit>()
            .map(|k| (k.data_root.clone(), k.key(name)));
        found
    }

    /// The music folder: the setting, else `~/Music`.
    #[must_use]
    pub fn music_folder(&self) -> PathBuf {
        let mut kit = self.kit.clone();
        let set = kit
            .downcast_ref::<kit::Kit>()
            .and_then(|k| k.settings.get(FOLDER_SETTING).map(PathBuf::from))
            .filter(|p| !p.as_os_str().is_empty());
        set.unwrap_or_else(scan::default_music_folder)
    }

    /// The library's track `id`.
    #[must_use]
    pub fn track(&self, id: &str) -> Option<Track> {
        self.with_library(|library| library.index_of(id).map(|i| library.tracks[i].clone()))
    }

    /// The track the listener hears now.
    #[must_use]
    pub fn heard(&self) -> Option<Track> {
        library_id(self, self.state.track).and_then(|id| self.track(&id))
    }

    /// The library's index of the track the listener hears now.
    #[must_use]
    pub fn heard_index(&self, library: &Library) -> Option<usize> {
        library_id(self, self.state.track).and_then(|id| library.index_of(&id))
    }

    /// The page the content shows, `columns` cards a row.
    #[must_use]
    pub fn page(&self, library: &Library, columns: usize) -> Page {
        let queue: Vec<String> = self
            .queue
            .up_next()
            .iter()
            .map(|id| (*id).to_string())
            .collect();
        page::build(
            &PageInput {
                view: &self.view,
                library,
                catalog: &self.catalog,
                playlists: &self.playlists,
                query: &self.query,
                queue: &queue,
                sort: self.sort,
            },
            columns,
        )
    }

    /// The page's songs in order.
    #[must_use]
    pub fn page_tracks(&self) -> Vec<usize> {
        self.with_library(|library| self.page(library, 1).tracks)
    }

    /// The library's track ids of `tracks` (library indices).
    #[must_use]
    pub fn ids_of(&self, tracks: &[usize]) -> Vec<String> {
        self.with_library(|library| {
            tracks
                .iter()
                .filter_map(|t| library.tracks.get(*t).map(|t| t.id.clone()))
                .collect()
        })
    }
}

// ==== Navigation ====

/// Shows `view`: from the sidebar (`remember` false: Back starts over) or from the page (a card,
/// an artist's name: Back returns here).
pub fn go(s: &mut Music, view: View, remember: bool) {
    if remember {
        if s.view != view {
            let here = std::mem::replace(&mut s.view, view);
            s.back.push(here);
            s.forward.clear();
        }
    } else {
        s.view = view;
        s.back.clear();
        s.forward.clear();
    }
    s.selected = None;
    s.hover = Hover::None;
}

/// Back to the page before (false: none).
pub fn go_back(s: &mut Music) -> bool {
    let Some(view) = s.back.pop() else {
        return false;
    };
    let here = std::mem::replace(&mut s.view, view);
    s.forward.push(here);
    s.selected = None;
    s.hover = Hover::None;
    true
}

/// Forward to the page Back left (false: none).
pub fn go_forward(s: &mut Music) -> bool {
    let Some(view) = s.forward.pop() else {
        return false;
    };
    let here = std::mem::replace(&mut s.view, view);
    s.back.push(here);
    s.selected = None;
    s.hover = Hover::None;
    true
}

/// The app's start: switches, the kit (settings, data root), the window.
pub fn start() {
    let args = match AppArgs::from_env(&SPEC) {
        Ok(a) => a,
        Err(message) => {
            println!("{message}");
            std::process::exit(if message.contains("USAGE") { 0 } else { 2 });
        }
    };
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &APP_CATEGORIES, args.clone());
    dark_by_default(&kit_ref);
    if args.screen.as_deref() == Some("settings") {
        kit::open_settings(&kit_ref, None);
    }
    let app = Music::new(kit_ref.clone(), args);
    let mut config = kit::app_config(&kit_ref);
    // The OS media keys and the desktop's now-playing widget.
    config.expose_system_media_controls = true;
    let window = kit::window_options(
        &kit_ref,
        crate::ui::layout,
        (1200.0, 760.0),
        // The sidebar, a page and the now-playing bar's transport, seek bar and volume.
        (960.0, 560.0),
        on_window_created,
    );
    App::create(RefAny::new(app), config).run(window);
}

/// AzMusic is dark until the user picks a mode (like Spotify): with no `--mode` and no settings
/// file yet, the mode is Dark (kept in the file with the first save); a mode the user picked -
/// light, dark or the system's - is followed.
fn dark_by_default(kit_ref: &RefAny) {
    let mut kit = kit_ref.clone();
    if let Some(mut k) = kit.downcast_mut::<kit::Kit>() {
        let saved = azul_appkit::data::local_path(&k.data_root, &k.settings_key()).exists();
        if k.args.mode.is_none() && !saved {
            k.settings.mode = ModePref::Dark;
        }
    };
}

// ==== The data: library.json and the playlists, through the Drive on a Thread ====

/// The window is up: the `--shot` timer, then the library and the playlists are read.
extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some((kit_ref, keys)) = data.downcast_ref::<Music>().map(|s| {
        (
            s.kit.clone(),
            s.root_and_key(LIBRARY_FILE)
                .zip(s.root_and_key(&format!("{PLAYLISTS_DIR}/"))),
        )
    }) else {
        return Update::DoNothing;
    };
    kit::on_window_created(&kit_ref, &mut info);
    if let Some(((root, library_key), (_, playlists_prefix))) = keys {
        kit::spawn_file_jobs(
            &mut info,
            &root,
            vec![
                FileJob::Get { key: library_key },
                FileJob::GetAll {
                    prefix: playlists_prefix,
                    suffix: String::from(".json"),
                },
            ],
            app,
            TAG_LOAD,
            on_files as WriteBackCallbackType,
        );
    }
    Update::DoNothing
}

/// A file job ended: the library and the playlists are in (or a save is done).
extern "C" fn on_files(mut data: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let app = data.clone();
    let Some(mut s) = data.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    if reply.tag == TAG_SAVE {
        for outcome in reply.outcomes {
            if let Some(why) = outcome.error() {
                s.status = format!("Not saved: {why}");
                println!("AZMUSIC_ERROR {why}");
            }
        }
        return Update::RefreshDom;
    }
    let mut library = Library::default();
    let mut playlists = Vec::new();
    for outcome in reply.outcomes {
        match outcome {
            FileOutcome::Got {
                result: Ok(Some(bytes)),
                ..
            } => match Library::from_json(&String::from_utf8_lossy(&bytes)) {
                Ok(l) => library = l,
                Err(why) => s.status = why,
            },
            FileOutcome::GotAll { files, .. } => {
                for (_, bytes) in files {
                    if let Ok(p) = Playlist::from_json(&String::from_utf8_lossy(&bytes)) {
                        playlists.push(p);
                    }
                }
            }
            _ => {}
        }
    }
    playlists.sort_by_key(|p| p.name.to_lowercase());
    s.playlists = playlists;
    println!(
        "AZMUSIC_LIBRARY {} {}",
        library.tracks.len(),
        library.albums().len()
    );
    let empty = library.tracks.is_empty();
    s.set_library(library);
    s.loaded = true;
    let sample = s.args.sample;
    drop(s);
    if empty && sample {
        write_sample(&app, &mut info);
    }
    Update::RefreshDom
}

/// Saves the library (and `playlists`) into the data tree, on a Thread.
fn save(app: &RefAny, s: &Music, info: &mut CallbackInfo, playlists: &[Playlist]) {
    let Some((root, key)) = s.root_and_key(LIBRARY_FILE) else {
        return;
    };
    let mut jobs = vec![FileJob::Put {
        key,
        bytes: s.library().to_json().into_bytes(),
    }];
    for p in playlists {
        if let Some((_, key)) = s.root_and_key(&format!("{PLAYLISTS_DIR}/{}", p.file_name())) {
            jobs.push(FileJob::Put {
                key,
                bytes: p.to_json().into_bytes(),
            });
        }
    }
    kit::spawn_file_jobs(
        info,
        &root,
        jobs,
        app.clone(),
        TAG_SAVE,
        on_files as WriteBackCallbackType,
    );
}

/// The sample library: six generated tones into `music/sample/` and a library of them.
pub fn write_sample(app: &RefAny, info: &mut CallbackInfo) {
    let mut app_ref = app.clone();
    let Some(mut s) = app_ref.downcast_mut::<Music>() else {
        return;
    };
    let Some((root, _)) = s.root_and_key("") else {
        return;
    };
    let mut jobs = Vec::new();
    let mut tracks = Vec::new();
    let now = now_s();
    for file in sample::sample_files() {
        let Some((_, key)) = s.root_and_key(&file.key) else {
            continue;
        };
        let mut track = file.track;
        track.id = azul_storage::ids::new_uuid();
        track.added_s = now;
        track.path = azul_appkit::data::local_path(&root, &key)
            .to_string_lossy()
            .into_owned();
        tracks.push(track);
        jobs.push(FileJob::Put {
            key,
            bytes: file.bytes,
        });
    }
    let library = Library {
        version: 1,
        folder: String::from("sample"),
        tracks,
    };
    println!(
        "AZMUSIC_LIBRARY {} {}",
        library.tracks.len(),
        library.albums().len()
    );
    s.set_library(library);
    s.loaded = true;
    s.status = String::from("The sample library: six generated tones.");
    if let Some((_, key)) = s.root_and_key(LIBRARY_FILE) {
        jobs.push(FileJob::Put {
            key,
            bytes: s.library().to_json().into_bytes(),
        });
    }
    drop(s);
    kit::spawn_file_jobs(
        info,
        &root,
        jobs,
        app.clone(),
        TAG_SAVE,
        on_files as WriteBackCallbackType,
    );
}

// ==== The scan of the music folder, on a Thread ====

struct ScanInit {
    folder: PathBuf,
    on_done: WriteBackCallbackType,
}

struct Scanned {
    found: usize,
    tracks: Vec<Track>,
}

/// Runs on the worker thread: every audio file's tags through azul's decoder.
extern "C" fn scan_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((folder, on_done)) = init
        .downcast_ref::<ScanInit>()
        .map(|i| (i.folder.clone(), i.on_done))
    else {
        return;
    };
    let files = scan::audio_files(&folder);
    let found = files.len();
    let mut tracks = Vec::with_capacity(found);
    for path in files {
        let path_text = path.to_string_lossy().into_owned();
        let decoder = AudioFileDecoder::open(AzString::from(path_text.as_str()));
        if !decoder.is_open() {
            continue;
        }
        let info = decoder.info();
        tracks.push(Track {
            id: String::new(),
            path: path_text,
            title: info.title.as_str().to_string(),
            artist: info.artist.as_str().to_string(),
            album: info.album.as_str().to_string(),
            album_artist: info.album_artist.as_str().to_string(),
            genre: info.genre.as_str().to_string(),
            year: info.date.as_str().chars().take(4).collect(),
            track_no: info.track_number,
            disc_no: info.disc_number,
            duration_s: info.duration_s,
            added_s: 0,
        });
    }
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(Scanned { found, tracks }),
    )));
}

/// Starts a scan of the music folder.
pub fn start_scan(app: &RefAny, info: &mut CallbackInfo) -> Update {
    let mut app_ref = app.clone();
    let Some(mut s) = app_ref.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    if s.scanning {
        return Update::DoNothing;
    }
    s.scanning = true;
    let folder = s.music_folder();
    s.status = format!("Scanning {}...", folder.display());
    drop(s);
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(ScanInit {
                folder,
                on_done: on_scanned as WriteBackCallbackType,
            }),
            app.clone(),
            scan_thread,
        ),
    );
    Update::RefreshDom
}

/// The scan is in: merged into the library (known files keep their ids), saved.
extern "C" fn on_scanned(mut data: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(scanned) = msg.downcast_mut::<Scanned>().map(|mut m| Scanned {
        found: m.found,
        tracks: std::mem::take(&mut m.tracks),
    }) else {
        return Update::DoNothing;
    };
    let app = data.clone();
    let Some(mut s) = data.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    let mut library = s.library();
    library.version = 1;
    library.folder = s.music_folder().to_string_lossy().into_owned();
    let (added, removed) = library.merge_scan(scanned.tracks, azul_storage::ids::new_uuid);
    let now = now_s();
    for t in &mut library.tracks {
        if t.added_s == 0 {
            t.added_s = now;
        }
    }
    library.tracks.sort_by(|a, b| {
        (
            a.filed_artist().to_lowercase(),
            a.album.to_lowercase(),
            a.disc_no,
            a.track_no,
        )
            .cmp(&(
                b.filed_artist().to_lowercase(),
                b.album.to_lowercase(),
                b.disc_no,
                b.track_no,
            ))
    });
    println!("AZMUSIC_SCAN {} {added} {removed}", scanned.found);
    let known: std::collections::HashSet<String> =
        library.tracks.iter().map(|t| t.id.clone()).collect();
    let mut playlists = s.playlists.clone();
    for p in &mut playlists {
        p.retain_known(|id| known.contains(id));
    }
    s.playlists = playlists.clone();
    let count = library.tracks.len();
    s.set_library(library);
    s.scanning = false;
    s.loaded = true;
    s.status = format!("{count} songs ({added} new, {removed} gone).");
    save(&app, &s, &mut info, &playlists);
    Update::RefreshDom
}

// ==== Playback ====

/// The player, made with the first play (at the volume the user set).
fn player(s: &mut Music) -> &AudioPlayer {
    let volume = s.volume;
    s.player.get_or_insert_with(|| {
        let p = AudioPlayer::create();
        p.set_volume(volume);
        p
    })
}

/// Seconds since 1970 (when a song came into the library).
fn now_s() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Plays the queue's current track now (what was handed over for later is dropped).
fn play_current(s: &mut Music, info: &mut CallbackInfo, app: &RefAny) {
    let Some(id) = s.queue.current().map(str::to_string) else {
        return;
    };
    let Some(track) = s.track(&id) else {
        return;
    };
    let player_id = player(s).load_file(AzString::from(track.path.as_str()));
    s.playing.push((player_id, id.clone()));
    s.queued_next = None;
    println!("AZMUSIC_PLAY {id} {}", track.display_title());
    ensure_ticking(s, info, app);
}

/// Hands the queue's next track to the player now (gapless), once per track.
fn queue_upcoming(s: &mut Music) {
    if s.queued_next.is_some() || s.player.is_none() || s.queue.current().is_none() {
        return;
    }
    let Some(next) = s.queue.upcoming().map(str::to_string) else {
        return;
    };
    let Some(track) = s.track(&next) else {
        return;
    };
    let player_id = player(s).queue_file(AzString::from(track.path.as_str()));
    s.playing.push((player_id, next));
    s.queued_next = Some(player_id);
}

/// The queue changed under the player: what was handed over for later is withdrawn.
pub fn requeue(s: &mut Music) {
    if let Some(p) = s.player.as_ref() {
        p.clear_queue();
    }
    s.queued_next = None;
    queue_upcoming(s);
}

/// The library track the player's track `player_id` plays.
#[must_use]
pub fn library_id(s: &Music, player_id: u64) -> Option<String> {
    s.playing
        .iter()
        .find(|(p, _)| *p == player_id)
        .map(|(_, id)| id.clone())
}

/// Starts the playback timer (once).
fn ensure_ticking(s: &mut Music, info: &mut CallbackInfo, app: &RefAny) {
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

/// What the desktop's now-playing widget shows.
fn now_playing(s: &Music) -> NowPlayingInfo {
    let heard = s.heard();
    let state = if s.state.finished || heard.is_none() {
        MediaPlaybackState::Stopped
    } else if s.state.playing {
        MediaPlaybackState::Playing
    } else {
        MediaPlaybackState::Paused
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ms = |seconds: f64| (seconds.max(0.0) * 1000.0) as u64;
    NowPlayingInfo {
        state,
        title: AzString::from(heard.as_ref().map(Track::display_title).unwrap_or_default()),
        artist: AzString::from(heard.as_ref().map(|t| t.artist.clone()).unwrap_or_default()),
        album: AzString::from(heard.as_ref().map(|t| t.album.clone()).unwrap_or_default()),
        artwork_url: AzString::from(""),
        duration_ms: ms(s.state.duration_s),
        position_ms: ms(s.state.position_s),
        volume: OptionF32::Some(s.state.volume),
    }
}

/// Every 250 ms: follow the player - the seek bar, the times and the meter in place; the queue
/// moves on at a track change (then the window is rebuilt and the desktop told).
extern "C" fn on_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(mut s) = data.downcast_mut::<Music>() else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let Some(state) = s.player.as_ref().map(AudioPlayer::get_state) else {
        return TimerCallbackReturn::continue_unchanged();
    };
    let before = s.state;
    s.state = state;
    let changed = state.track != before.track
        || state.playing != before.playing
        || state.finished != before.finished;
    if state.track != before.track && state.track != 0 && Some(state.track) == s.queued_next {
        // The handed-over track is heard: the queue moves on, the one after it is handed over.
        s.queue.advance();
        s.queued_next = None;
    }
    queue_upcoming(&mut s);
    if changed {
        if let Some(t) = s.heard() {
            println!(
                "AZMUSIC_HEARD {} {:.1}",
                t.display_title(),
                state.position_s
            );
        }
        let word = if state.finished {
            "finished"
        } else if state.playing {
            "playing"
        } else {
            "paused"
        };
        println!("AZMUSIC_STATE {word}");
        let np = now_playing(&s);
        info.callback_info.set_now_playing(np);
    }
    // The seek bar, the time played and the length, and the meter, in place.
    if let Some(seek) = info
        .callback_info
        .get_node_id_by_marker(ids::SEEK)
        .into_option()
    {
        SeekBar::update_position(info.callback_info, seek, state.position_s);
    }
    let elapsed = SeekBar::media_time(state.position_s).as_str().to_string();
    if elapsed != s.elapsed {
        if let Some(node) = info
            .callback_info
            .get_node_id_by_marker(ids::ELAPSED)
            .into_option()
        {
            info.callback_info
                .change_node_text(node, AzString::from(elapsed.as_str()));
        }
        s.elapsed = elapsed;
    }
    let total = SeekBar::media_time(state.duration_s).as_str().to_string();
    if total != s.total {
        if let Some(node) = info
            .callback_info
            .get_node_id_by_marker(ids::TOTAL)
            .into_option()
        {
            info.callback_info
                .change_node_text(node, AzString::from(total.as_str()));
        }
        s.total = total;
    }
    let level = LevelMeter::peak_level(state.peak_left.max(state.peak_right)).round();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let now_ms = (state.position_s.max(0.0) * 1000.0) as u64;
    if let Some(level) = s.meter.next(level, now_ms).into_option() {
        if let Some(meter) = info
            .callback_info
            .get_node_id_by_marker(ids::LEVEL)
            .into_option()
        {
            LevelMeter::update_level(info.callback_info, meter, level);
        }
    }
    if changed {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
    }
}

/// Plays the page's songs from row `row` on, in the page's order (an album, a playlist, the songs
/// as sorted, the search's); the queue holds the whole page, so Previous goes back up it.
pub fn play_page_from(s: &mut Music, info: &mut CallbackInfo, app: &RefAny, row: usize) {
    let tracks = s.page_tracks();
    let ids = s.ids_of(&tracks);
    let start = row.min(ids.len().saturating_sub(1));
    play_ids(s, info, app, ids, start);
}

/// Plays the songs `tracks` (library indices) from the first.
pub fn play_tracks(s: &mut Music, info: &mut CallbackInfo, app: &RefAny, tracks: &[usize]) {
    let ids = s.ids_of(tracks);
    play_ids(s, info, app, ids, 0);
}

/// Plays the songs `tracks` shuffled: shuffle on, a song picked at random first.
pub fn shuffle_tracks(s: &mut Music, info: &mut CallbackInfo, app: &RefAny, tracks: &[usize]) {
    let ids = s.ids_of(tracks);
    if ids.is_empty() {
        return;
    }
    #[allow(clippy::cast_possible_truncation)]
    let start = (seed() % ids.len() as u64) as usize;
    let repeat = s.queue.repeat;
    s.queue = PlayQueue::new(ids, start);
    s.queue.repeat = repeat;
    s.queue.set_shuffle(true, seed());
    play_current(s, info, app);
}

/// Plays `ids` from `start`, keeping shuffle and repeat.
pub fn play_ids(
    s: &mut Music,
    info: &mut CallbackInfo,
    app: &RefAny,
    ids: Vec<String>,
    start: usize,
) {
    if ids.is_empty() {
        return;
    }
    let (shuffle, repeat) = (s.queue.shuffle, s.queue.repeat);
    s.queue = PlayQueue::new(ids, start);
    s.queue.repeat = repeat;
    if shuffle {
        s.queue.set_shuffle(true, seed());
    }
    play_current(s, info, app);
}

/// `track` (an id) plays right after the song that plays (at once when nothing does).
pub fn play_next(s: &mut Music, info: &mut CallbackInfo, app: &RefAny, track: &str) {
    let idle = s.queue.current().is_none();
    s.queue.play_next(track.to_string());
    if idle {
        play_current(s, info, app);
    } else {
        requeue(s);
    }
}

/// `track` (an id) plays after everything queued (at once when nothing plays).
pub fn enqueue(s: &mut Music, info: &mut CallbackInfo, app: &RefAny, track: &str) {
    let idle = s.queue.current().is_none();
    s.queue.enqueue(track.to_string());
    if idle {
        play_current(s, info, app);
    } else {
        requeue(s);
    }
}

/// A seed for the shuffle: the clock.
#[allow(clippy::cast_possible_truncation)]
fn seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64)
}

/// One transport action (the bar's buttons, the keys, the media keys); `value` is the volume
/// for `Volume`.
pub fn transport(
    s: &mut Music,
    info: &mut CallbackInfo,
    app: &RefAny,
    action: MediaControlsAction,
    value: f32,
) {
    match action {
        MediaControlsAction::PlayPause => {
            if s.queue.current().is_none() {
                let row = s.selected.unwrap_or(0);
                play_page_from(s, info, app, row);
            } else if s.player.is_none() || s.state.finished {
                play_current(s, info, app);
            } else if let Some(p) = s.player.as_ref() {
                p.toggle();
            }
        }
        MediaControlsAction::Next => {
            if s.queue.skip().is_some() {
                play_current(s, info, app);
            } else if let Some(p) = s.player.as_ref() {
                p.stop();
            }
        }
        MediaControlsAction::Previous => match s.queue.previous(s.state.position_s) {
            Previous::Restart => {
                if let Some(p) = s.player.as_ref() {
                    p.seek(0.0);
                }
            }
            Previous::Track(_) => play_current(s, info, app),
        },
        MediaControlsAction::SkipBack => {
            if let Some(p) = s.player.as_ref() {
                p.seek((s.state.position_s - 15.0).max(0.0));
            }
        }
        MediaControlsAction::SkipForward => {
            if let Some(p) = s.player.as_ref() {
                p.seek(s.state.position_s + 30.0);
            }
        }
        MediaControlsAction::Shuffle => {
            let on = !s.queue.shuffle;
            s.queue.set_shuffle(on, seed());
            requeue(s);
        }
        MediaControlsAction::Repeat => {
            s.queue.repeat = match s.queue.repeat {
                Repeat::Off => Repeat::All,
                Repeat::All => Repeat::One,
                Repeat::One => Repeat::Off,
            };
            requeue(s);
        }
        MediaControlsAction::Volume => {
            let value = value.clamp(0.0, 1.0);
            if let Some(p) = s.player.as_ref() {
                p.set_volume(value);
            }
            s.volume = value;
            s.state.volume = value;
        }
    }
}

/// Mute, or back to the volume before the mute.
pub fn toggle_mute(s: &mut Music, info: &mut CallbackInfo, app: &RefAny) {
    let value = if s.volume > 0.0 {
        s.unmuted = s.volume;
        0.0
    } else {
        s.unmuted.max(0.1)
    };
    transport(s, info, app, MediaControlsAction::Volume, value);
}

/// The seek bar: while the thumb is dragged only the bar moves; the player seeks on release.
pub extern "C" fn on_seek(mut data: RefAny, _info: CallbackInfo, state: SeekBarState) -> Update {
    let Some(s) = data.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    if !state.dragging {
        if let Some(p) = s.player.as_ref() {
            p.seek(state.position_s);
        }
    }
    Update::DoNothing
}

/// Re-renders the page (its VirtualView) in place - no `layout()`, no relayout of the window:
/// the pointer moved to another song or card, a song was selected, the songs were sorted.
pub fn rerender_page(info: &mut CallbackInfo) {
    let Some(node) = info.get_node_id_by_marker(ids::CONTENT).into_option() else {
        return;
    };
    // `into_raw` is the 1-based encoding (0 = none); `NodeId` is 0-based.
    let raw = node.node.into_raw();
    if raw != 0 {
        info.trigger_virtual_view_rerender(node.dom, NodeId { inner: raw - 1 });
    }
}

/// Whether the keyboard focus is in a text field (the search, a field of the settings page):
/// its keys - Space, Enter, Escape - are its own.
fn in_text_field(info: &CallbackInfo) -> bool {
    let Some(mut node) = info.get_focused_node().into_option() else {
        return false;
    };
    for _ in 0..6 {
        let classes = info.get_node_classes(node);
        if classes
            .as_slice()
            .iter()
            .any(|c| c.as_str().contains("text-input"))
        {
            return true;
        }
        match info.get_parent(node).into_option() {
            Some(parent) => node = parent,
            None => return false,
        }
    }
    false
}

/// Puts the caret in the search field.
fn focus_search(info: &mut CallbackInfo) {
    let dom = DomId { inner: 0 };
    let node = info.get_node_id_by_id_attribute(dom, ids::SEARCH);
    if node.into_raw() != 0 {
        info.set_focus(FocusTarget::Id(DomNodeId { dom, node }));
    }
}

/// The window's keys: the kit's first, then the app's - the player's (and the media keys), Back
/// and Forward, Enter on the selected song, Mod+F to search.
pub extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(kit_ref) = data.downcast_ref::<Music>().map(|s| s.kit.clone()) else {
        return Update::DoNothing;
    };
    if let Some(update) = kit::handle_key(&kit_ref, &mut info) {
        return update;
    }
    let Some(key) = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option()
    else {
        return Update::DoNothing;
    };
    let primary = info.get_key_modifiers().primary_down();
    let typing = in_text_field(&info);
    if key == VirtualKeyCode::R && primary {
        info.prevent_default();
        return start_scan(&app, &mut info);
    }
    if key == VirtualKeyCode::F && primary {
        info.prevent_default();
        focus_search(&mut info);
        return Update::DoNothing;
    }
    let Some(mut s) = data.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    let moved = match key {
        VirtualKeyCode::LBracket if primary => Some(go_back(&mut s)),
        VirtualKeyCode::RBracket if primary => Some(go_forward(&mut s)),
        VirtualKeyCode::Escape if !typing => Some(go_back(&mut s)),
        _ => None,
    };
    if let Some(moved) = moved {
        if !moved {
            return Update::DoNothing;
        }
        info.prevent_default();
        return Update::RefreshDom;
    }
    if matches!(key, VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) && !typing {
        let Some(row) = s.selected else {
            return Update::DoNothing;
        };
        info.prevent_default();
        play_page_from(&mut s, &mut info, &app, row);
        return Update::RefreshDom;
    }
    let volume = s.volume;
    let action = match key {
        VirtualKeyCode::PlayPause => Some((MediaControlsAction::PlayPause, 0.0)),
        // Space plays / pauses unless a text field (the search) takes the typing.
        VirtualKeyCode::Space if !typing => Some((MediaControlsAction::PlayPause, 0.0)),
        VirtualKeyCode::NextTrack => Some((MediaControlsAction::Next, 0.0)),
        VirtualKeyCode::PrevTrack => Some((MediaControlsAction::Previous, 0.0)),
        VirtualKeyCode::Right if primary => Some((MediaControlsAction::Next, 0.0)),
        VirtualKeyCode::Left if primary => Some((MediaControlsAction::Previous, 0.0)),
        VirtualKeyCode::Up if primary => {
            Some((MediaControlsAction::Volume, (volume + 0.1).min(1.0)))
        }
        VirtualKeyCode::Down if primary => {
            Some((MediaControlsAction::Volume, (volume - 0.1).max(0.0)))
        }
        VirtualKeyCode::MediaStop => {
            if let Some(p) = s.player.as_ref() {
                p.stop();
            }
            return Update::RefreshDom;
        }
        _ => None,
    };
    match action {
        Some((action, value)) => {
            info.prevent_default();
            transport(&mut s, &mut info, &app, action, value);
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

/// The desktop's media widget asked for a seek or a volume.
pub extern "C" fn on_media_control(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(request) = info.get_media_control_request().into_option() else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    let position = s.state.position_s;
    #[allow(clippy::cast_precision_loss)]
    let offset = request.position_us as f64 / 1e6;
    match request.kind {
        MediaControlKind::SeekAbsolute => {
            if let Some(p) = s.player.as_ref() {
                p.seek(offset.max(0.0));
            }
        }
        MediaControlKind::SeekRelative => {
            if let Some(p) = s.player.as_ref() {
                p.seek((position + offset).max(0.0));
            }
        }
        MediaControlKind::SetVolume => {
            if let Some(p) = s.player.as_ref() {
                p.set_volume(request.volume);
            }
            s.volume = request.volume;
            s.state.volume = request.volume;
        }
        MediaControlKind::OpenUri => {}
    }
    Update::DoNothing
}

// ==== Playlists ====

/// "Playlist <n>" of `tracks` (ids), into the list (by name).
fn new_playlist(s: &mut Music, tracks: &[String]) -> Playlist {
    let mut playlist = Playlist::new(
        azul_storage::ids::new_uuid(),
        format!("Playlist {}", s.playlists.len() + 1),
    );
    playlist.add(tracks);
    s.playlists.push(playlist.clone());
    s.playlists.sort_by_key(|p| p.name.to_lowercase());
    playlist
}

/// "New Playlist": the queue from the song that plays on (or nothing) becomes a playlist, saved
/// as a file, and opens.
pub extern "C" fn on_new_playlist(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(mut s) = data.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    let tracks: Vec<String> = s
        .queue
        .up_next()
        .iter()
        .map(|id| (*id).to_string())
        .collect();
    let playlist = new_playlist(&mut s, &tracks);
    go(&mut s, View::Playlist(playlist.id.clone()), false);
    s.status = format!(
        "Made \u{201c}{}\u{201d} ({}).",
        playlist.name,
        page::count(playlist.tracks.len(), "song")
    );
    println!("AZMUSIC_PLAYLIST {} {}", playlist.tracks.len(), playlist.name);
    save(&app, &s, &mut info, &[playlist]);
    Update::RefreshDom
}

/// Adds the song `track` (an id) to the playlist `id` - to a new playlist when `id` is `None` -
/// and saves it.
pub fn add_to_playlist(
    app: &RefAny,
    s: &mut Music,
    info: &mut CallbackInfo,
    id: Option<&str>,
    track: &str,
) {
    let tracks = [track.to_string()];
    let saved = match id {
        Some(id) => match s.playlists.iter_mut().find(|p| p.id == id) {
            Some(p) => {
                p.add(&tracks);
                p.clone()
            }
            None => return,
        },
        None => new_playlist(s, &tracks),
    };
    s.status = format!("Added to \u{201c}{}\u{201d}.", saved.name);
    println!("AZMUSIC_PLAYLIST {} {}", saved.tracks.len(), saved.name);
    save(app, s, info, &[saved]);
}

/// Takes the song in row `row` of the playlist `id`'s page out of it (the row counts the songs
/// still in the library) and saves it.
pub fn remove_from_playlist(
    app: &RefAny,
    s: &mut Music,
    info: &mut CallbackInfo,
    id: &str,
    row: usize,
) {
    let entry = s.with_library(|library| {
        s.playlists.iter().find(|p| p.id == id).and_then(|p| {
            p.tracks
                .iter()
                .enumerate()
                .filter(|(_, t)| library.index_of(t).is_some())
                .nth(row)
                .map(|(i, _)| i)
        })
    });
    let Some(entry) = entry else {
        return;
    };
    let Some(p) = s.playlists.iter_mut().find(|p| p.id == id) else {
        return;
    };
    p.remove_at(entry);
    let saved = p.clone();
    s.selected = None;
    s.hover = Hover::None;
    println!("AZMUSIC_PLAYLIST {} {}", saved.tracks.len(), saved.name);
    save(app, s, info, &[saved]);
}
