//! The app's state, its start, the data (library and playlists through the Drive), the scan of
//! the music folder and playback (the queue and azul's `AudioPlayer`). The window is `ui.rs`.

use std::path::PathBuf;

use azul::{
    audio::{AudioFileDecoder, AudioPlayer, AudioPlayerState, MediaPlaybackState, NowPlayingInfo},
    callbacks::{TimerCallbackInfo, TimerCallbackReturn, WriteBackCallbackType},
    dom::VirtualKeyCode,
    misc::MediaControlKind,
    option::OptionF32,
    prelude::*,
    str::String as AzString,
    task::{
        Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg,
        Timer, TimerId,
    },
    time::{Duration, SystemTimeDiff},
    widgets::{
        DataTableEditTarget, DataTableView, LevelMeter, LevelMeterThrottle, MediaControlsAction,
        MediaControlsEvent, SeekBar, SeekBarState,
    },
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec},
    files::{FileJob, FileOutcome},
    shortcuts::Shortcut,
    ui as kit,
};

use crate::{
    ids,
    library::{Library, Track, LIBRARY_FILE},
    playlists::{Playlist, PLAYLISTS_DIR},
    queue::{PlayQueue, Previous, Repeat},
    sample, scan,
};

// ==== The app's facts ====

/// The screens `--screen` opens.
pub const SCREENS: [&str; 4] = ["songs", "albums", "artists", "settings"];

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
pub const SHORTCUTS: [Shortcut; 7] = [
    Shortcut::new("Playback", "Space", "Play / pause"),
    Shortcut::new("Playback", "Mod+Right  Mod+Left", "Next / previous track"),
    Shortcut::new("Playback", "Mod+Up  Mod+Down", "Volume up / down"),
    Shortcut::new(
        "Playback",
        "Media keys",
        "Play, pause, next, previous, stop",
    ),
    Shortcut::new("Library", "Enter  Double-click", "Play the song from here"),
    Shortcut::new("Library", "Mod+F", "Filter the songs"),
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

/// What the content shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    Songs,
    Albums,
    Artists,
    /// A playlist, by id.
    Playlist(String),
}

/// The app's state.
pub struct Music {
    /// The appkit kit (settings, data root, the settings page's state).
    pub kit: RefAny,
    /// The library (shared with the songs table's data callback).
    pub library: RefAny,
    /// The playlists, by name.
    pub playlists: Vec<Playlist>,
    pub view: View,
    /// The songs table's view (sort, filter, selection, scroll).
    pub table: DataTableView,
    pub queue: PlayQueue,
    /// The player, made with the first play.
    pub player: Option<AudioPlayer>,
    /// The player's track ids and the library's track ids they play.
    pub playing: Vec<(u64, String)>,
    /// The player's id of the track handed over for gapless playback (the queue's next).
    pub queued_next: Option<u64>,
    /// What the player said last.
    pub state: AudioPlayerState,
    /// The meter's throttle.
    pub meter: LevelMeterThrottle,
    /// The playback timer runs.
    pub ticking: bool,
    /// The library file was read (or found missing).
    pub loaded: bool,
    pub scanning: bool,
    /// The status line.
    pub status: String,
    /// The window's size (the table's viewport follows it).
    pub window: (f32, f32),
    pub args: AppArgs,
}

/// The library, as the songs table's data source sees it.
pub struct LibraryRef {
    pub library: Library,
}

impl Music {
    fn new(kit: RefAny, args: AppArgs) -> Self {
        let view = match args.screen.as_deref() {
            Some("albums") => View::Albums,
            Some("artists") => View::Artists,
            _ => View::Songs,
        };
        Self {
            kit,
            library: RefAny::new(LibraryRef {
                library: Library::default(),
            }),
            playlists: Vec::new(),
            view,
            table: DataTableView::create(),
            queue: PlayQueue::default(),
            player: None,
            playing: Vec::new(),
            queued_next: None,
            state: AudioPlayerState::default(),
            meter: LevelMeterThrottle::create(100),
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
        let mut r = self.library.clone();
        let copy = r
            .downcast_ref::<LibraryRef>()
            .map(|l| l.library.clone())
            .unwrap_or_default();
        copy
    }

    fn set_library(&mut self, library: Library) {
        let mut r = self.library.clone();
        let guard = r.downcast_mut::<LibraryRef>();
        if let Some(mut l) = guard {
            l.library = library;
        }
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
        let library = self.library();
        library.index_of(id).map(|i| library.tracks[i].clone())
    }

    /// The track the listener hears now.
    #[must_use]
    pub fn heard(&self) -> Option<Track> {
        library_id(self, self.state.track).and_then(|id| self.track(&id))
    }
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
        (720.0, 480.0),
        on_window_created,
    );
    App::create(RefAny::new(app), config).run(window);
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
    s.table = DataTableView::create();
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
    for file in sample::sample_files() {
        let Some((_, key)) = s.root_and_key(&file.key) else {
            continue;
        };
        let mut track = file.track;
        track.id = azul_storage::ids::new_uuid();
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
    s.table = DataTableView::create();
    s.status = format!("{count} songs ({added} new, {removed} gone).");
    save(&app, &s, &mut info, &playlists);
    Update::RefreshDom
}

// ==== Playback ====

/// The player, made with the first play.
fn player(s: &mut Music) -> &AudioPlayer {
    s.player.get_or_insert_with(AudioPlayer::create)
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
fn requeue(s: &mut Music) {
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

/// Every 250 ms: follow the player - the seek bar and the meter in place; the queue moves on at a
/// track change (then the window is rebuilt and the desktop told).
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
    // The seek bar and the meter, in place.
    if let Some(seek) = info
        .callback_info
        .get_node_id_by_marker(ids::SEEK)
        .into_option()
    {
        SeekBar::update_position(info.callback_info, seek, state.position_s);
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

/// Plays the songs shown in the table from position `row` on (the table's order).
pub fn play_from_table(s: &mut Music, info: &mut CallbackInfo, app: &RefAny, row: u32) {
    let library = s.library();
    let rows = u32::try_from(library.tracks.len()).unwrap_or(u32::MAX);
    let shown = s.table.shown_count(rows);
    let ids: Vec<String> = (0..shown)
        .filter_map(|p| s.table.row_at(p, rows).into_option())
        .filter_map(|r| library.tracks.get(r as usize).map(|t| t.id.clone()))
        .collect();
    let start = (0..shown)
        .position(|p| s.table.row_at(p, rows).into_option() == Some(row))
        .unwrap_or(0);
    play_ids(s, info, app, ids, start);
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

/// A seed for the shuffle: the clock.
#[allow(clippy::cast_possible_truncation)]
fn seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64)
}

/// One transport action (the controls, the keys, the media keys).
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
                play_from_table(s, info, app, 0);
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
            if let Some(p) = s.player.as_ref() {
                p.set_volume(value);
            }
            s.state.volume = value;
        }
    }
}

/// The now-playing bar's controls.
pub extern "C" fn on_controls(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: MediaControlsEvent,
) -> Update {
    let app = data.clone();
    let Some(mut s) = data.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    transport(&mut s, &mut info, &app, event.action, event.value);
    if event.action == MediaControlsAction::Volume {
        Update::DoNothing
    } else {
        Update::RefreshDom
    }
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

/// The window's keys: the kit's first, then the player's (and the media keys).
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
    if key == VirtualKeyCode::R && primary {
        info.prevent_default();
        return start_scan(&app, &mut info);
    }
    let Some(mut s) = data.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    let volume = s.state.volume;
    let action = match key {
        VirtualKeyCode::PlayPause => Some((MediaControlsAction::PlayPause, 0.0)),
        // Space plays / pauses unless the songs' filter row takes the typing.
        VirtualKeyCode::Space if s.table.edit == DataTableEditTarget::None => {
            Some((MediaControlsAction::PlayPause, 0.0))
        }
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
            s.state.volume = request.volume;
        }
        MediaControlKind::OpenUri => {}
    }
    Update::DoNothing
}

/// The queue (from the current track on) becomes a playlist, saved as a file.
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
    if tracks.is_empty() {
        s.status = String::from("Play something first: the queue becomes the playlist.");
        return Update::RefreshDom;
    }
    let mut playlist = Playlist::new(
        azul_storage::ids::new_uuid(),
        format!("Playlist {}", s.playlists.len() + 1),
    );
    playlist.add(&tracks);
    s.view = View::Playlist(playlist.id.clone());
    s.playlists.push(playlist.clone());
    s.status = format!(
        "Saved \"{}\" ({} songs).",
        playlist.name,
        playlist.tracks.len()
    );
    save(&app, &s, &mut info, &[playlist]);
    Update::RefreshDom
}
