//! What plays: music (a queue on azul's `AudioPlayer`, gapless: the next song is handed over
//! while one plays), a video (azul's `VideoWidget` for the picture, the same `AudioPlayer` for
//! its sound, opened behind the curtain - `curtain.rs` - so both start together), the picture
//! viewer and the slide show (Ken Burns' pan and zoom, a cross-fade from one picture to the
//! next). The transport - one [`Command`] for every button, menu item and key - is here too.

use azul::{
    audio::AudioPlayerState,
    callbacks::SeekBarOnSeekCallbackType,
    dialog::{FileDialog, FileOpenResult},
    dom::VirtualKeyCode,
    error::ResultUrlUrlParseError,
    file::FileTypeList,
    option::{OptionFileTypeList, OptionString},
    prelude::*,
    str::String as AzString,
    url::Url,
    vec::StringVec,
    video::{VideoPhase, VideoStatus},
    widgets::{SeekBar, SeekBarState},
    window::WindowFrame,
};

use crate::{
    app::{self, Player},
    curtain::{Cue, Curtain},
    ids,
    library::{Item, Shelf},
    pages::Screen,
    sync::{volume_osd, SyncGuard, DEAD_BAND_S},
};

/// How far the rewind / fast-forward buttons jump, seconds.
pub const REWIND_S: f64 = 10.0;
pub const FORWARD_S: f64 = 30.0;
/// "Previous" within this many seconds of a song's start goes to the song before; later it
/// starts the song again.
pub const RESTART_WITHIN_S: f64 = 3.0;
/// A slide shows this long before the next comes.
pub const SLIDE_S: u64 = 6;
/// The history is written at most this often while a video plays (media seconds).
pub const SAVE_EVERY_S: f64 = 10.0;

// ==== Music ====

/// The music that plays: the queue and where it is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Music {
    /// The songs, in play order (paths: the library may be scanned again under the queue).
    pub paths: Vec<String>,
    /// The song heard now.
    pub index: usize,
    /// The player's track ids and the songs they are.
    pub ids: Vec<(u64, usize)>,
    /// The song handed to the player to follow the one heard (gapless).
    pub queued_next: Option<usize>,
    /// What the player said last.
    pub state: AudioPlayerState,
    /// The whole second the time label shows (-1: none yet).
    pub elapsed_shown: i64,
    /// The songs were shuffled.
    pub shuffled: bool,
}

impl Music {
    /// The path of the song heard now.
    #[must_use]
    pub fn current(&self) -> Option<&str> {
        self.paths.get(self.index).map(String::as_str)
    }

    /// The library's item of the song heard now (its tags; `None` for a file not in it).
    #[must_use]
    pub fn current_item<'a>(&self, s: &'a Player) -> Option<&'a Item> {
        let path = self.current()?;
        s.items(Shelf::Music).iter().find(|i| i.path == path)
    }

    /// Whether the music plays (not paused, not finished).
    #[must_use]
    pub fn playing(&self) -> bool {
        self.state.playing
    }
}

/// A seed for a shuffle: the clock's nanoseconds.
fn seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(7, |d| u64::from(d.subsec_nanos()) ^ d.as_secs())
}

/// `items` in a shuffled order (xorshift, Fisher-Yates), `first` kept first when given.
#[must_use]
pub fn shuffled(mut items: Vec<String>, first: Option<usize>, seed: u64) -> Vec<String> {
    let mut x = seed | 1;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let head = first.and_then(|i| (i < items.len()).then(|| items.remove(i)));
    for i in (1..items.len()).rev() {
        #[allow(clippy::cast_possible_truncation)]
        let j = (next() % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
    if let Some(head) = head {
        items.insert(0, head);
    }
    items
}

/// Plays `paths` from `start` (a video that plays is closed first; the page goes to now
/// playing when `show` is set).
pub fn play_music(app: &RefAny, info: &mut CallbackInfo, paths: Vec<String>, start: usize, show: bool) {
    if paths.is_empty() {
        return;
    }
    close_video(app, info);
    let mut app_ref = app.clone();
    let Some(mut guard) = app_ref.downcast_mut::<Player>() else {
        return;
    };
    let s = &mut *guard;
    let start = start.min(paths.len() - 1);
    let id = s.player().load_file(AzString::from(paths[start].as_str()));
    println!("AZPLAYER_MUSIC play {}", crate::library::file_title(std::path::Path::new(&paths[start])));
    s.music = Some(Music {
        paths,
        index: start,
        ids: vec![(id, start)],
        queued_next: None,
        state: AudioPlayerState::default(),
        elapsed_shown: -1,
        shuffled: false,
    });
    queue_next(s);
    if show && s.place().screen != Screen::NowPlaying {
        crate::nav::go(s, Screen::NowPlaying);
    }
}

/// Hands the player the song after the one heard (gapless), once.
fn queue_next(s: &mut Player) {
    let Some((next, path)) = s.music.as_ref().and_then(|m| {
        let next = m.index + 1;
        (m.queued_next.is_none() && next < m.paths.len()).then(|| (next, m.paths[next].clone()))
    }) else {
        return;
    };
    let id = s.player().queue_file(AzString::from(path.as_str()));
    if let Some(m) = s.music.as_mut() {
        m.ids.push((id, next));
        m.queued_next = Some(next);
    }
}

/// Goes to song `index` of the queue now.
fn jump_to(s: &mut Player, index: usize) {
    let Some(path) = s.music.as_ref().and_then(|m| m.paths.get(index).cloned()) else {
        return;
    };
    let id = s.player().load_file(AzString::from(path.as_str()));
    if let Some(m) = s.music.as_mut() {
        m.index = index;
        m.ids = vec![(id, index)];
        m.queued_next = None;
        m.elapsed_shown = -1;
    }
    println!("AZPLAYER_MUSIC play {}", crate::library::file_title(std::path::Path::new(&path)));
    queue_next(s);
}

/// The music's tick: which song is heard (the queue moves on, the next is handed over), the end,
/// and - in place - the seek bar and the time on the now-playing page. `true` = rebuild.
pub fn tick_music(s: &mut Player, info: &mut CallbackInfo) -> bool {
    let Some(state) = s.music.as_ref().and(s.audio.as_ref()).map(|a| a.get_state()) else {
        return false;
    };
    let mut rebuild = false;
    let mut advanced = false;
    // A state from before the player took this queue (the last session's) says nothing.
    let mut known = false;
    if let Some(m) = s.music.as_mut() {
        if let Some(&(_, index)) = m.ids.iter().find(|(id, _)| *id == state.track) {
            known = true;
            if index != m.index {
                m.index = index;
                m.queued_next = None;
                m.elapsed_shown = -1;
                advanced = true;
            }
        }
        if !known {
            return false;
        }
        let was_playing = m.state.playing;
        m.state = state;
        if was_playing != state.playing {
            println!(
                "AZPLAYER_MUSIC {}",
                if state.playing { "playing" } else { "paused" }
            );
            rebuild = true;
        }
    }
    if advanced {
        if let Some(path) = s.music.as_ref().and_then(Music::current) {
            println!("AZPLAYER_MUSIC play {}", crate::library::file_title(std::path::Path::new(path)));
        }
        queue_next(s);
        rebuild = true;
    }
    if known && state.finished {
        println!("AZPLAYER_MUSIC finished");
        s.music = None;
        return true;
    }
    // In place: the seek bar and the time played (once a second).
    let position = state.position_s.max(0.0);
    if let Some(seek) = info.get_node_id_by_marker(ids::SEEK).into_option() {
        SeekBar::update_position(*info, seek, position);
    }
    #[allow(clippy::cast_possible_truncation)]
    let whole = position.floor() as i64;
    if let Some(m) = s.music.as_mut() {
        if whole != m.elapsed_shown {
            m.elapsed_shown = whole;
            let text = SeekBar::media_time(position);
            app::set_text(info, ids::ELAPSED, text.as_str());
        }
    }
    rebuild
}

// ==== A video ====

/// A video being opened or played.
#[derive(Debug, Clone)]
pub struct VideoSession {
    /// Its file (or address).
    pub path: String,
    /// Asked of the video widget: the last seek target, in seconds.
    pub seek_s: f32,
    /// Asked of the video widget: hold (`true`) or play. Held while the curtain is down.
    pub paused: bool,
    /// What the video widget last reported.
    pub status: VideoStatus,
    /// The opening: the preroll, the fade to black, the picture fading in.
    pub curtain: Curtain,
    /// The sound's track id in the audio player (0: none).
    pub audio_id: u64,
    /// When the sound is moved to the picture.
    pub sync: SyncGuard,
    /// The whole second the time label shows (-1: none yet).
    pub elapsed_shown: i64,
    /// The media time of the last history write.
    pub saved_at_s: f64,
}

impl VideoSession {
    /// The file's title (its name without the folder and the extension).
    #[must_use]
    pub fn title(&self) -> String {
        crate::library::file_title(std::path::Path::new(&self.path))
    }

    /// Whether the picture plays (as the widget says and as it is asked).
    #[must_use]
    pub fn playing(&self) -> bool {
        self.status.phase == VideoPhase::Playing && !self.paused
    }
}

/// The web address `text` names, when it is one AzPlayer plays (http or https): the video is
/// then read by range requests as it plays, its sound through the same download.
#[must_use]
pub fn web_address(text: &str) -> Option<Url> {
    let text = text.trim();
    if !(text.starts_with("http://") || text.starts_with("https://")) {
        return None;
    }
    match Url::parse(AzString::from(text)) {
        ResultUrlUrlParseError::Ok(url) if url.is_http() || url.is_https() => Some(url),
        _ => None,
    }
}

/// Opens `path` - a file or a web address: the picture from where it was left, paused and
/// HIDDEN, and the sound preloaded at the same place and held; the menus stay until both are
/// ready (the curtain, in the tick).
pub fn open_video(app: &RefAny, info: &mut CallbackInfo, path: &str) {
    // An address as the parser writes it: the picture and the sound then share one download.
    let address = web_address(path);
    let path = address
        .as_ref()
        .map_or_else(|| path.to_string(), |u| u.href.as_str().to_string());
    let path = path.as_str();
    close_video(app, info);
    let mut app_ref = app.clone();
    let Some(mut guard) = app_ref.downcast_mut::<Player>() else {
        return;
    };
    let s = &mut *guard;
    // Music stops for a video (Media Center's way).
    if s.music.take().is_some() {
        if let Some(a) = s.audio.as_ref() {
            a.stop();
        }
    }
    s.viewer.playing = false;
    let now_s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    s.history.touch(path, now_s);
    // Where it was left - unless the settings say every video starts at its beginning.
    let resume = if s.options.is_on(crate::options::RESUME) {
        s.history.resume_at(path)
    } else {
        0.0
    };
    #[allow(clippy::cast_possible_truncation)]
    let seek_s = resume as f32;
    let audio_id = match address {
        Some(url) => s.player().preload_url(url.href, resume),
        None => s.player().preload_file(AzString::from(path), resume),
    };
    let now = s.now_ms();
    s.video = Some(VideoSession {
        path: path.to_string(),
        seek_s,
        paused: true,
        status: VideoStatus {
            message: AzString::from(""),
            position_s: seek_s,
            duration_s: 0.0,
            phase: VideoPhase::Loading,
        },
        curtain: Curtain::preroll(now),
        audio_id,
        sync: SyncGuard::default(),
        elapsed_shown: -1,
        saved_at_s: resume,
    });
    if s.place().screen != Screen::Video {
        crate::nav::go(s, Screen::Video);
    }
    s.activity();
    s.controls_shown = true;
    println!("AZPLAYER_OPEN {path} {resume:.1}");
    println!("AZPLAYER_CURTAIN preroll");
    app::save_history(app, s, info);
}

/// Closes the video (its position is kept); the page under it comes back.
pub fn close_video(app: &RefAny, info: &mut CallbackInfo) {
    let mut app_ref = app.clone();
    let Some(mut guard) = app_ref.downcast_mut::<Player>() else {
        return;
    };
    let s = &mut *guard;
    let Some(video) = s.video.take() else {
        return;
    };
    let (position, duration) = (
        f64::from(video.status.position_s),
        f64::from(video.status.duration_s),
    );
    if video.status.phase != VideoPhase::Failed {
        s.history.set_position(&video.path, position, duration);
    }
    if let Some(a) = s.audio.as_ref() {
        a.stop();
    }
    if s.place().screen == Screen::Video {
        s.nav.pop();
    }
    println!("AZPLAYER_CLOSE {position:.1}");
    app::save_history(app, s, info);
}

/// The video's tick: is the sound ready (the picture says so itself, `on_video_status`), the
/// curtain's next step. `true` = rebuild.
pub fn tick_video(s: &mut Player, _info: &mut CallbackInfo, now: u64) -> bool {
    let Some(audio_state) = s.video.as_ref().and(s.audio.as_ref()).map(|a| a.get_state()) else {
        return false;
    };
    let Some(video) = s.video.as_mut() else {
        return false;
    };
    if let Curtain::Preroll { sound: false, .. } = video.curtain {
        let id = video.audio_id;
        let ready = (audio_state.track == id && audio_state.buffered_s > 0.0)
            || audio_state.failed_track == id
            || id == 0;
        if ready {
            video.curtain.sound_ready();
            if audio_state.failed_track == id || id == 0 {
                println!("AZPLAYER_PREROLL sound none");
            } else {
                println!("AZPLAYER_PREROLL sound {:.3}", audio_state.buffered_s);
            }
        }
    }
    match video.curtain.step(now) {
        Some(Cue::FadeOut) => {
            println!("AZPLAYER_CURTAIN fade-out");
            true
        }
        Some(Cue::Play) => {
            // Picture and sound start together: the widget is told by the rebuild, the player
            // now.
            video.paused = false;
            video.sync.reset();
            if let Some(a) = s.audio.as_ref() {
                a.play();
            }
            println!("AZPLAYER_CURTAIN play");
            println!(
                "AZPLAYER_AUDIO {} {:.3}",
                if audio_state.track == video.audio_id { "ready" } else { "none" },
                audio_state.buffered_s
            );
            true
        }
        Some(Cue::Opened) => {
            println!("AZPLAYER_CURTAIN open");
            true
        }
        None => false,
    }
}

/// The video widget reports where it is (about four times a second while it plays): the first
/// picture lifts the preroll's half of the curtain; the sound follows the picture (only a
/// lasting drift, see [`SyncGuard`]); the seek bar and the time move in place; the position is
/// remembered. Only a new phase rebuilds the window.
pub extern "C" fn on_video_status(
    mut data: RefAny,
    mut info: CallbackInfo,
    status: VideoStatus,
) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<Player>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let now_ms = s.now_ms();
    let Some(video) = s.video.as_mut() else {
        return Update::DoNothing;
    };
    let phase_changed = status.phase != video.status.phase;
    let duration_changed = (status.duration_s - video.status.duration_s).abs() > 0.01;
    video.status = status.clone();
    let position = f64::from(status.position_s);
    let word = match status.phase {
        VideoPhase::Loading => "loading",
        VideoPhase::Paused => "paused",
        VideoPhase::Playing => "playing",
        VideoPhase::Ended => "ended",
        VideoPhase::Failed => "failed",
    };
    if phase_changed {
        println!("AZPLAYER_STATE {word} {position:.1}");
    }
    let mut rebuild = phase_changed;
    // The first picture is shown (held): the picture's half of the preroll.
    if video.curtain.prerolling() && status.phase == VideoPhase::Paused {
        if let Curtain::Preroll { picture: false, .. } = video.curtain {
            println!("AZPLAYER_PREROLL picture {position:.1}");
        }
        video.curtain.picture_ready();
    }
    if status.phase == VideoPhase::Failed && video.curtain.active() && video.curtain != Curtain::Open
    {
        video.curtain.failed();
        rebuild = true;
    }
    if let Some(audio) = s.audio.as_ref() {
        match status.phase {
            VideoPhase::Playing if !video.paused => {
                let heard = audio.get_state();
                if heard.track == video.audio_id && !heard.playing && !heard.finished {
                    audio.play();
                }
                if heard.track == video.audio_id && heard.track != 0 && !heard.finished {
                    if let Some(target) = video.sync.correct(position, heard.position_s, now_ms) {
                        audio.seek(target);
                    }
                }
            }
            // The picture waits for its download (a stall): the sound waits with it, and plays
            // on with it (the arm above).
            VideoPhase::Loading if video.curtain == Curtain::Open && !video.paused => {
                audio.pause();
            }
            VideoPhase::Ended | VideoPhase::Failed => audio.pause(),
            _ => {}
        }
    }
    // Remember where it is, now and then (and at the end: from the start next time).
    let ended = status.phase == VideoPhase::Ended;
    let mut save = false;
    if ended || (position - video.saved_at_s).abs() >= SAVE_EVERY_S {
        let at = if ended { 0.0 } else { position };
        let path = video.path.clone();
        video.saved_at_s = position;
        s.history.set_position(&path, at, f64::from(status.duration_s));
        save = true;
    }
    if save {
        app::save_history(&app, s, &mut info);
    }
    if rebuild {
        return Update::RefreshDom;
    }
    // In place: the seek bar, the time played (once a second), the length when it is known.
    if let Some(seek) = info.get_node_id_by_marker(ids::SEEK).into_option() {
        SeekBar::update_position(info, seek, position);
    }
    #[allow(clippy::cast_possible_truncation)]
    let whole = position.max(0.0).floor() as i64;
    if let Some(video) = s.video.as_mut() {
        if whole != video.elapsed_shown {
            video.elapsed_shown = whole;
            let text = SeekBar::media_time(position.max(0.0));
            app::set_text(&mut info, ids::ELAPSED, text.as_str());
        }
    }
    if duration_changed {
        let text = SeekBar::media_time(f64::from(status.duration_s));
        app::set_text(&mut info, ids::TOTAL, text.as_str());
    }
    Update::DoNothing
}

/// Seeks the picture and the sound of the video to `seconds`.
fn seek_video(s: &mut Player, seconds: f64) {
    let Some(video) = s.video.as_mut() else {
        return;
    };
    let duration = f64::from(video.status.duration_s);
    let target = if duration > 0.0 {
        seconds.clamp(0.0, duration)
    } else {
        seconds.max(0.0)
    };
    #[allow(clippy::cast_possible_truncation)]
    let mut t = target as f32;
    // The widget seeks when its timestamp CHANGES: a second seek to the same place moves a
    // millisecond so it is a change.
    if (t - video.seek_s).abs() < f32::EPSILON {
        t += 0.001;
    }
    video.seek_s = t;
    video.sync.reset();
    let jump = (f64::from(video.status.position_s) - target).abs() > DEAD_BAND_S;
    if let Some(a) = s.audio.as_ref() {
        a.seek(f64::from(t));
    }
    if jump {
        let text = SeekBar::media_time(target).as_str().to_string();
        s.osd(text);
    }
}

/// Back (negative) or forward `seconds` in what plays: the video, else the music.
pub fn seek_by(app: &RefAny, info: &mut CallbackInfo, seconds: f64) -> Update {
    let mut app_ref = app.clone();
    let Some(mut guard) = app_ref.downcast_mut::<Player>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    s.activity();
    if let Some(position) = s.video.as_ref().map(|v| f64::from(v.status.position_s)) {
        seek_video(s, position + seconds);
    } else if let Some(position) = s.music.as_ref().map(|m| m.state.position_s) {
        let target = (position + seconds).max(0.0);
        if let Some(a) = s.audio.as_ref() {
            a.seek(target);
        }
        let text = SeekBar::media_time(target).as_str().to_string();
        s.osd(text);
    } else {
        return Update::DoNothing;
    }
    app::show_osd(s, info);
    Update::RefreshDom
}

/// The seek bar: the picture and the sound seek on release (a drag only moves the bar); music
/// seeks the same way.
pub extern "C" fn on_seek(mut data: RefAny, _info: CallbackInfo, state: SeekBarState) -> Update {
    let Some(mut s) = data.downcast_mut::<Player>() else {
        return Update::DoNothing;
    };
    s.activity();
    if state.dragging {
        return Update::DoNothing;
    }
    if s.video.is_some() {
        seek_video(&mut s, state.position_s);
    } else if s.music.is_some() {
        if let Some(a) = s.audio.as_ref() {
            a.seek(state.position_s);
        }
    }
    Update::RefreshDom
}

/// Opens the file dialog for a video.
pub extern "C" fn on_open(data: RefAny, _info: CallbackInfo) -> Update {
    let filter = OptionFileTypeList::Some(FileTypeList {
        document_types: StringVec::from_vec(vec![
            AzString::from("mp4"),
            AzString::from("m4v"),
            AzString::from("mov"),
        ]),
        document_descriptor: AzString::from("Video"),
    });
    let _request =
        FileDialog::open_file("Open a video", OptionString::None, filter, data, on_picked);
    Update::DoNothing
}

extern "C" fn on_picked(data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing;
    };
    open_video(&data, &mut info, path.inner.as_str());
    Update::RefreshDom
}

// ==== The picture viewer and the slide show ====

/// The pictures being looked at.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Viewer {
    pub paths: Vec<String>,
    /// The picture shown.
    pub index: usize,
    /// The slide show runs.
    pub playing: bool,
    /// When the picture shown came (ms on the app's clock).
    pub since_ms: u64,
    /// How many pictures came since the viewer opened (each a slide of its own, its own Ken
    /// Burns).
    pub shown: usize,
}

/// Shows `paths` from `index`; `slideshow` starts the slide show.
pub fn show_pictures(s: &mut Player, paths: Vec<String>, index: usize, slideshow: bool) {
    if paths.is_empty() {
        return;
    }
    let now = s.now_ms();
    s.viewer = Viewer {
        index: index.min(paths.len() - 1),
        paths,
        playing: slideshow,
        since_ms: now,
        shown: 0,
    };
    if s.place().screen != Screen::Picture {
        crate::nav::go(s, Screen::Picture);
    }
    s.activity();
    println!(
        "AZPLAYER_PICTURE {} {}",
        s.viewer.index,
        if slideshow { "slideshow" } else { "still" }
    );
}

/// The next (`+1`) or the previous (`-1`) picture, round.
pub fn step_picture(s: &mut Player, by: isize) {
    let n = s.viewer.paths.len();
    if n == 0 {
        return;
    }
    let index = (s.viewer.index as isize + by).rem_euclid(n as isize);
    #[allow(clippy::cast_sign_loss)]
    {
        s.viewer.index = index as usize;
    }
    s.viewer.shown += 1;
    s.viewer.since_ms = s.now_ms();
    println!("AZPLAYER_PICTURE {} {}", s.viewer.index, if s.viewer.playing { "slideshow" } else { "still" });
}

/// How long a slide shows (the settings'), seconds.
#[must_use]
pub fn slide_s(s: &Player) -> u64 {
    s.options.seconds(crate::options::SLIDE_SECONDS).max(1)
}

/// How far the skip buttons and Left / Right jump (the settings'), seconds: back is negative.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn skip_s(s: &Player, forward: bool) -> f64 {
    if forward {
        s.options.seconds(crate::options::SKIP_FORWARD) as f64
    } else {
        -(s.options.seconds(crate::options::SKIP_BACK) as f64)
    }
}

/// The slide show's tick: the next picture once one has shown long enough AND the next is
/// decoded (a slow picture never shows half made). `(rebuild, wants pictures)`.
pub fn tick_viewer(s: &mut Player, now: u64) -> (bool, bool) {
    if s.place().screen != Screen::Picture || !s.viewer.playing || s.viewer.paths.is_empty() {
        return (false, false);
    }
    if now.saturating_sub(s.viewer.since_ms) < slide_s(s) * 1000 {
        return (false, false);
    }
    let next = (s.viewer.index + 1) % s.viewer.paths.len();
    let ready = s.art.contains_key(&app::full_key(&s.viewer.paths[next]));
    if !ready {
        return (false, true);
    }
    step_picture(s, 1);
    (true, true)
}

// ==== The transport: one command for every button, menu item and key ====

/// What a button, a menu item or a key asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// The file dialog.
    Open,
    /// Back a page.
    Back,
    /// The start strip.
    Home,
    /// The settings page.
    Settings,
    /// The about box.
    About,
    PlayPause,
    /// Held at the start (a video), or the music stopped.
    Stop,
    /// From the start.
    Restart,
    /// The song or picture before.
    Previous,
    /// The song or picture after.
    Next,
    /// Back [`REWIND_S`].
    Rewind,
    /// Forward [`FORWARD_S`].
    Forward,
    Mute,
    VolumeUp,
    VolumeDown,
    Fullscreen,
    /// The rest of the music in another order.
    Shuffle,
    /// The slide show on / off.
    SlideShow,
    /// Music under the slide show (every song, shuffled) on / off.
    SlideMusic,
    /// Scan the libraries again.
    Refresh,
    /// Close AzPlayer.
    Quit,
}

/// A button's or a menu item's payload: the app and what it asks for.
pub struct CommandRef {
    pub app: RefAny,
    pub command: Command,
}

/// A button or a menu item.
pub extern "C" fn on_command(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((app, command)) = data
        .downcast_ref::<CommandRef>()
        .map(|c| (c.app.clone(), c.command))
    else {
        return Update::DoNothing;
    };
    run(&app, &mut info, command)
}

/// Fullscreen on / off: the window's frame (and the system kept awake while it is).
fn set_fullscreen(s: &mut Player, info: &mut CallbackInfo, on: bool) {
    s.fullscreen = on;
    let mut ws = info.get_current_window_state();
    ws.flags.frame = if on {
        WindowFrame::Fullscreen
    } else {
        WindowFrame::Normal
    };
    ws.flags.prevent_system_sleep = on;
    info.modify_window_state(ws);
    println!("AZPLAYER_FULLSCREEN {}", if on { "on" } else { "off" });
}

fn set_volume(s: &mut Player, volume: f32) {
    s.volume = volume.clamp(0.0, 1.0);
    s.muted = false;
    if let Some(a) = s.audio.as_ref() {
        a.set_volume(s.volume);
    }
    let text = volume_osd(s.volume, false);
    s.osd(text);
}

/// What plays now: a video, the music, the slide show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Playing {
    Video,
    Music,
    Pictures,
    Nothing,
}

/// What the transport acts on: the video, else the slide show (on its page), else the music.
#[must_use]
pub fn playing(s: &Player) -> Playing {
    if s.video.is_some() {
        Playing::Video
    } else if s.place().screen == Screen::Picture {
        Playing::Pictures
    } else if s.music.is_some() {
        Playing::Music
    } else {
        Playing::Nothing
    }
}

/// Whether the chrome over what plays may hide itself (it stays while a video is paused or
/// still opening).
#[must_use]
pub fn chrome_hides(s: &Player) -> bool {
    match &s.video {
        Some(v) => v.curtain == Curtain::Open && v.playing(),
        None => true,
    }
}

/// Does `command`. The volume changes only the OSD (in place); the rest rebuilds the window once.
#[allow(clippy::too_many_lines)]
pub fn run(app: &RefAny, info: &mut CallbackInfo, command: Command) -> Update {
    match command {
        Command::Open => return on_open(app.clone(), *info),
        Command::Back => return crate::nav::back(app, info),
        Command::Home => return crate::nav::home(app, info),
        // The settings are pages of the media center (`settings.rs`), never the desktop's
        // Options dialog: a video closes first.
        Command::Settings | Command::About => {
            close_video(app, info);
            let mut app_ref = app.clone();
            if let Some(mut s) = app_ref.downcast_mut::<Player>() {
                if command == Command::About {
                    crate::settings::open_category(&mut s, crate::options::Category::About);
                } else {
                    crate::settings::open(&mut s);
                }
            }
            return Update::RefreshDom;
        }
        Command::Refresh => {
            app::scan_all(app, info);
            let mut app_ref = app.clone();
            if let Some(mut s) = app_ref.downcast_mut::<Player>() {
                s.notice("Looking through the folders again.");
            }
            return Update::RefreshDom;
        }
        Command::Quit => {
            close_video(app, info);
            println!("AZPLAYER_QUIT");
            info.close_window();
            return Update::DoNothing;
        }
        Command::SlideMusic => return slide_music(app, info),
        _ => {}
    }
    let mut app_ref = app.clone();
    let Some(mut guard) = app_ref.downcast_mut::<Player>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    s.activity();
    let what = playing(s);
    let mut rebuild = true;
    match (command, what) {
        (Command::PlayPause, Playing::Video) => {
            // Behind the curtain the opening decides when it plays.
            let open = s.video.as_ref().is_some_and(|v| v.curtain == Curtain::Open);
            if open {
                let ended = s
                    .video
                    .as_ref()
                    .is_some_and(|v| v.status.phase == VideoPhase::Ended);
                if ended {
                    if let Some(v) = s.video.as_mut() {
                        v.paused = false;
                    }
                    seek_video(s, 0.0);
                } else if let Some(v) = s.video.as_mut() {
                    v.paused = !v.paused;
                }
                let paused = s.video.as_ref().is_none_or(|v| v.paused);
                if let Some(v) = s.video.as_mut() {
                    v.sync.reset();
                }
                if let Some(a) = s.audio.as_ref() {
                    if paused {
                        a.pause();
                    } else {
                        a.play();
                    }
                }
            }
        }
        (Command::PlayPause, Playing::Music) => {
            if let Some(a) = s.audio.as_ref() {
                a.toggle();
            }
        }
        (Command::PlayPause | Command::SlideShow, Playing::Pictures) => {
            s.viewer.playing = !s.viewer.playing;
            s.viewer.since_ms = s.now_ms();
            println!(
                "AZPLAYER_SLIDESHOW {}",
                if s.viewer.playing { "playing" } else { "paused" }
            );
        }
        (Command::Stop, Playing::Video) => {
            if let Some(video) = s.video.as_mut() {
                video.paused = true;
            }
            if let Some(a) = s.audio.as_ref() {
                a.pause();
            }
            seek_video(s, 0.0);
            s.osd(String::from("Stopped"));
        }
        (Command::Stop, Playing::Music) => {
            if let Some(a) = s.audio.as_ref() {
                a.stop();
            }
            s.music = None;
            println!("AZPLAYER_MUSIC stopped");
        }
        (Command::Stop, Playing::Pictures) => {
            s.viewer.playing = false;
        }
        (Command::Restart, Playing::Video) => seek_video(s, 0.0),
        (Command::Restart, Playing::Music) => {
            if let Some(a) = s.audio.as_ref() {
                a.seek(0.0);
            }
        }
        (Command::Rewind | Command::Forward, Playing::Video) => {
            let position = s
                .video
                .as_ref()
                .map_or(0.0, |v| f64::from(v.status.position_s));
            let by = skip_s(s, command == Command::Forward);
            seek_video(s, position + by);
        }
        (Command::Rewind | Command::Forward, Playing::Music) => {
            let position = s.music.as_ref().map_or(0.0, |m| m.state.position_s);
            let by = skip_s(s, command == Command::Forward);
            let target = (position + by).max(0.0);
            if let Some(a) = s.audio.as_ref() {
                a.seek(target);
            }
            let text = SeekBar::media_time(target).as_str().to_string();
            s.osd(text);
        }
        (Command::Previous, Playing::Music) => {
            let (position, index) = s
                .music
                .as_ref()
                .map_or((0.0, 0), |m| (m.state.position_s, m.index));
            if position > RESTART_WITHIN_S || index == 0 {
                if let Some(a) = s.audio.as_ref() {
                    a.seek(0.0);
                }
            } else {
                jump_to(s, index - 1);
            }
        }
        (Command::Next, Playing::Music) => {
            let (index, len) = s
                .music
                .as_ref()
                .map_or((0, 0), |m| (m.index, m.paths.len()));
            if index + 1 < len {
                jump_to(s, index + 1);
            } else {
                s.notice("That was the last song.");
            }
        }
        (Command::Previous, Playing::Pictures) => step_picture(s, -1),
        (Command::Next, Playing::Pictures) => step_picture(s, 1),
        (Command::Previous, Playing::Video) => seek_video(s, 0.0),
        (Command::Shuffle, Playing::Music) => {
            if let Some(m) = s.music.as_ref() {
                let index = m.index;
                let paths = m.paths.clone();
                let shuffled_paths = shuffled(paths, Some(index), seed());
                let mut music = s.music.take().unwrap_or_default();
                music.paths = shuffled_paths;
                music.index = 0;
                // The song heard stays; the one queued after it is the new order's.
                music.ids.retain(|(_, i)| *i == index);
                for entry in &mut music.ids {
                    entry.1 = 0;
                }
                music.queued_next = None;
                music.shuffled = true;
                s.music = Some(music);
                if let Some(a) = s.audio.as_ref() {
                    a.clear_queue();
                }
                queue_next(s);
                s.osd(String::from("Shuffled"));
            }
        }
        (Command::Mute, _) => {
            s.muted = !s.muted;
            if let Some(a) = s.audio.as_ref() {
                a.set_volume(if s.muted { 0.0 } else { s.volume });
            }
            let text = volume_osd(s.volume, s.muted);
            s.osd(text);
            rebuild = true;
        }
        (Command::VolumeUp, _) => {
            let v = s.volume + 0.1;
            set_volume(s, v);
            rebuild = false;
        }
        (Command::VolumeDown, _) => {
            let v = s.volume - 0.1;
            set_volume(s, v);
            rebuild = false;
        }
        (Command::Fullscreen, _) => {
            let on = !s.fullscreen;
            set_fullscreen(s, info, on);
        }
        (_, Playing::Nothing) => {
            s.notice("Nothing is playing.");
        }
        _ => {
            rebuild = false;
        }
    }
    // The chrome and the OSD as the state has them, in place - also what a rebuild then keeps.
    app::set_chrome(s, info, true);
    app::show_osd(s, info);
    drop(guard);
    if rebuild {
        // A picture stepped to: its window-sized copy (and the next one's).
        app::request_art(app, info);
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// Music under the slide show: every song, shuffled, on; or off.
fn slide_music(app: &RefAny, info: &mut CallbackInfo) -> Update {
    let paths = {
        let mut app_ref = app.clone();
        let Some(mut s) = app_ref.downcast_mut::<Player>() else {
            return Update::DoNothing;
        };
        if s.music.is_some() {
            if let Some(a) = s.audio.as_ref() {
                a.stop();
            }
            s.music = None;
            return Update::RefreshDom;
        }
        let paths: Vec<String> = s
            .items(Shelf::Music)
            .iter()
            .map(|i| i.path.clone())
            .collect();
        if paths.is_empty() {
            s.notice("There is no music in the music folder.");
            return Update::RefreshDom;
        }
        shuffled(paths, None, seed())
    };
    play_music(app, info, paths, 0, false);
    Update::RefreshDom
}

/// The keys of what plays (any page): Space, M, the volume (Up / Down only while something
/// plays on a page without a gallery), the media keys. `None`: not a transport key here.
#[must_use]
pub fn transport_key(s: &Player, key: VirtualKeyCode, gallery: bool) -> Option<Command> {
    let nothing = playing(s) == Playing::Nothing;
    match key {
        VirtualKeyCode::Space | VirtualKeyCode::PlayPause if !nothing => Some(Command::PlayPause),
        VirtualKeyCode::MediaStop if !nothing => Some(Command::Stop),
        VirtualKeyCode::NextTrack => Some(Command::Next),
        VirtualKeyCode::PrevTrack => Some(Command::Previous),
        VirtualKeyCode::M => Some(Command::Mute),
        VirtualKeyCode::Up if !gallery && !nothing => Some(Command::VolumeUp),
        VirtualKeyCode::Down if !gallery && !nothing => Some(Command::VolumeDown),
        VirtualKeyCode::Equals | VirtualKeyCode::Plus | VirtualKeyCode::NumpadAdd => {
            Some(Command::VolumeUp)
        }
        VirtualKeyCode::Minus | VirtualKeyCode::NumpadSubtract => Some(Command::VolumeDown),
        VirtualKeyCode::F11 => Some(Command::Fullscreen),
        VirtualKeyCode::F if !gallery => Some(Command::Fullscreen),
        VirtualKeyCode::PageUp if !nothing => Some(Command::Previous),
        VirtualKeyCode::PageDown if !nothing => Some(Command::Next),
        _ => None,
    }
}

/// The seek bar's callback, typed.
pub const ON_SEEK: SeekBarOnSeekCallbackType = on_seek;

// ==== The round buttons: the transport, the top band ====

/// One round button of the transport.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransportButton {
    pub command: Command,
    pub icon: &'static str,
    /// Its name for assistive technology.
    pub name: &'static str,
    pub size: f32,
    /// Its id's name (`ids::id`).
    pub id: &'static str,
    /// A gap before it (the volume after the transport, the music after the slide show's).
    pub gap: bool,
}

/// The transport that shows for `screen`, if one does: the music's on now playing, the slide
/// show's on the picture viewer, the video's over a video.
#[must_use]
pub fn transport_of(screen: &Screen) -> Option<Shelf> {
    match screen {
        Screen::NowPlaying => Some(Shelf::Music),
        Screen::Picture => Some(Shelf::Pictures),
        Screen::Video => Some(Shelf::Videos),
        _ => None,
    }
}

/// The transport's buttons, left to right: for music shuffle, previous, rewind, play / pause,
/// forward, next, stop and the volume; for a video stop, from the start, rewind, play / pause,
/// forward and the volume; for the slide show previous, play / pause, next and the music.
#[must_use]
pub fn transport_buttons(s: &Player, what: Shelf) -> Vec<TransportButton> {
    let playing = match what {
        Shelf::Music => s.music.as_ref().is_some_and(Music::playing),
        Shelf::Pictures => s.viewer.playing,
        _ => s
            .video
            .as_ref()
            .is_some_and(|v| !v.paused && v.status.phase != VideoPhase::Ended),
    };
    let (play_icon, play_name) = if playing {
        ("pause", "Pause")
    } else {
        ("play_arrow", "Play")
    };
    let (mute_icon, mute_name) = if s.muted {
        ("volume_off", "Sound on")
    } else {
        ("volume_up", "Mute")
    };
    let b = |command: Command,
             icon: &'static str,
             name: &'static str,
             size: f32,
             id: &'static str| TransportButton {
        command,
        icon,
        name,
        size,
        id,
        gap: false,
    };
    let mut out = match what {
        Shelf::Pictures => {
            let music_name = if s.music.is_some() {
                "Stop the music"
            } else {
                "Music under the slide show"
            };
            let show_name = if playing {
                "Pause the slide show"
            } else {
                "Play the slide show"
            };
            vec![
                b(Command::Previous, "skip_previous", "Previous picture", 40.0, "previous"),
                b(Command::SlideShow, play_icon, show_name, 54.0, "play"),
                b(Command::Next, "skip_next", "Next picture", 40.0, "next"),
                TransportButton {
                    gap: true,
                    ..b(Command::SlideMusic, "library_music", music_name, 34.0, "music")
                },
            ]
        }
        Shelf::Music => vec![
            b(Command::Shuffle, "shuffle", "Shuffle", 34.0, "shuffle"),
            b(Command::Previous, "skip_previous", "Previous song", 40.0, "previous"),
            b(Command::Rewind, "fast_rewind", "Skip back", 40.0, "rewind"),
            b(Command::PlayPause, play_icon, play_name, 56.0, "play"),
            b(Command::Forward, "fast_forward", "Skip forward", 40.0, "forward"),
            b(Command::Next, "skip_next", "Next song", 40.0, "next"),
            b(Command::Stop, "stop", "Stop", 40.0, "stop"),
        ],
        _ => vec![
            b(Command::Stop, "stop", "Stop", 40.0, "stop"),
            b(Command::Restart, "skip_previous", "From the start", 40.0, "restart"),
            b(Command::Rewind, "fast_rewind", "Skip back", 40.0, "rewind"),
            b(Command::PlayPause, play_icon, play_name, 56.0, "play"),
            b(Command::Forward, "fast_forward", "Skip forward", 40.0, "forward"),
        ],
    };
    if what != Shelf::Pictures {
        out.push(TransportButton {
            gap: true,
            ..b(Command::Mute, mute_icon, mute_name, 34.0, "mute")
        });
        out.push(b(Command::VolumeDown, "remove", "Volume down", 34.0, "volume-down"));
        out.push(b(Command::VolumeUp, "add", "Volume up", 34.0, "volume-up"));
    }
    out
}

/// The transport button the keys are on: the one Left / Right moved to, else play / pause.
#[must_use]
pub fn transport_focus(buttons: &[TransportButton], chosen: Option<usize>) -> usize {
    match chosen {
        Some(i) if i < buttons.len() => i,
        _ => buttons
            .iter()
            .position(|b| matches!(b.command, Command::PlayPause | Command::SlideShow))
            .unwrap_or(0),
    }
}

/// The top band's buttons on `screen`: back, and over a video fullscreen.
#[must_use]
pub fn corner_buttons(screen: &Screen) -> Vec<Command> {
    if *screen == Screen::Video {
        vec![Command::Back, Command::Fullscreen]
    } else {
        vec![Command::Back]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shuffle_keeps_every_song_once_and_the_one_heard_first() {
        let songs: Vec<String> = (0..20).map(|i| format!("/m/{i}.mp3")).collect();
        let out = shuffled(songs.clone(), Some(7), 12345);
        assert_eq!(out.len(), 20);
        assert_eq!(out[0], "/m/7.mp3", "the song heard stays first");
        let mut sorted = out.clone();
        sorted.sort();
        let mut expected = songs.clone();
        expected.sort();
        assert_eq!(sorted, expected, "every song once");
        assert_ne!(out, songs, "another order");
        assert_eq!(shuffled(songs.clone(), None, 9), shuffled(songs, None, 9), "steady per seed");
        assert!(shuffled(Vec::new(), Some(3), 1).is_empty());
    }
}
