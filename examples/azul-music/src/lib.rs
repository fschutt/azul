//! AzMusic: a music player on the public azul API.
//!
//! The window is azul's S7 `MediaShell`: the library in the sidebar (Songs, Albums, Artists, the
//! playlists), the content (the songs in azul's virtualized `DataTable` with its filter row; the
//! albums and the artists as lists - `TODO(WIDGETS9A): IconGrid` for album covers), and the
//! now-playing bar always under both: the track, azul's `MediaControls`, `SeekBar` and
//! `LevelMeter`. The title row is azul's `Titlebar` (the window is `NoTitle`).
//!
//! Playback is azul's `AudioPlayer` (it decodes on its own thread, plays gaplessly): the app keeps
//! the queue ([`queue::PlayQueue`]) and hands the player the NEXT file while the current one plays,
//! so albums run on without a gap. A 250 ms timer reads what is heard (`AudioPlayer::get_state`)
//! and moves the seek bar and the meter in place (`SeekBar::update_position`,
//! `LevelMeter::update_level`) - the window is rebuilt only when the track or the state changes.
//! The OS media keys and the desktop's now-playing widget work (`set_now_playing`, the
//! media-control event).
//!
//! The data (the S3 split): `music/library.json` (the tracks and their tags) and
//! `music/playlists/<id>.json` in the data tree, through the azul-storage Drive on a Thread; the
//! music itself is read where it is (the music folder). `--sample` (or the empty state's button)
//! writes six generated tones into `music/sample/` and a library of them.
//!
//! Switches (azul-appkit): `--screen songs|albums|artists|settings`, `--theme flat|flora`, `--mode
//! light|dark|system`, `--size WxH`, `--shot <png>`, `--sample`, `--data-dir <dir>`.
//!
//! On stdout, for scripts (`scripts/azmusic_e2e.py`): `AZMUSIC_LIBRARY <tracks> <albums>`,
//! `AZMUSIC_SCAN <found> <added> <removed>`, `AZMUSIC_PLAY <track id> <title>`,
//! `AZMUSIC_HEARD <title> <position>`, `AZMUSIC_STATE <playing|paused|finished>`,
//! `AZMUSIC_ERROR <message>`.

pub mod ids;
pub mod library;
pub mod playlists;
pub mod queue;
pub mod sample;
pub mod scan;

pub mod app;
pub mod ui;

pub use app::start;
