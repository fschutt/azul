//! AzMusic: a music player on the public azul API, in the look of Spotify's 2010 desktop player.
//!
//! The window (`ui.rs`, colours in `look.rs`), in the player's own sans in every theme, none of
//! it selectable text but the search field: the tool bar - back, forward, the search field, the
//! status - which IS the title bar (the window is `NoTitle`: the bar moves the window, a double
//! click zooms, the window's controls keep their corner); the sidebar - Play Queue, the library
//! (Recently Added, Artists, Albums, Songs, Genres), the playlists and "New Playlist", the cover
//! of the song that plays at its foot; the page; the now-playing bar - the round transport, the
//! song, azul's `SeekBar` between the times, shuffle, repeat, the queue, the volume (`Slider`)
//! and azul's `LevelMeter`. Dark by default, the app's mode once the user picks one.
//!
//! The page is a VirtualView (`page.rs` says what its lines are; only those in view are built):
//! grids of album covers, round artist initials and genre tiles; an album's header (cover, title,
//! artist, year, Play, Shuffle) over its songs; an artist's albums, each cover beside its songs;
//! dense striped song tables with sortable columns. A song under the pointer shows a play icon
//! (the page is re-rendered in place, the window is not rebuilt); a double-click plays from it, a
//! right click offers Play Next, Add to Queue and Add to Playlist. Covers are the album's
//! initials on a colour of its own (`art.rs`): the library keeps no pictures.
//!
//! Playback is azul's `AudioPlayer` (it decodes on its own thread, plays gaplessly): the app keeps
//! the queue ([`queue::PlayQueue`]) and hands the player the NEXT file while the current one plays,
//! so albums run on without a gap. A 250 ms timer reads what is heard (`AudioPlayer::get_state`)
//! and moves the seek bar, the times and the meter in place (`SeekBar::update_position`,
//! `CallbackInfo::change_node_text`, `LevelMeter::update_level`) - the window is rebuilt only when
//! the track or the state changes. The OS media keys and the desktop's now-playing widget work
//! (`set_now_playing`, the media-control event).
//!
//! The data (the S3 split): `music/library.json` (the tracks and their tags) and
//! `music/playlists/<id>.json` in the data tree, through the azul-storage Drive on a Thread; the
//! music itself is read where it is (the music folder). `--sample` (or the empty state's button)
//! writes six generated tones into `music/sample/` and a library of them.
//!
//! Switches (azul-appkit): `--screen recent|songs|albums|artists|genres|settings`, `--theme
//! flat|flora`, `--mode light|dark|system`, `--size WxH`, `--shot <png>`, `--sample`,
//! `--data-dir <dir>`.
//!
//! On stdout, for scripts (`scripts/azmusic_e2e.py`): `AZMUSIC_LIBRARY <tracks> <albums>`,
//! `AZMUSIC_SCAN <found> <added> <removed>`, `AZMUSIC_PLAY <track id> <title>`,
//! `AZMUSIC_HEARD <title> <position>`, `AZMUSIC_STATE <playing|paused|finished>`,
//! `AZMUSIC_SEARCH <songs found>`, `AZMUSIC_PLAYLIST <songs> <name>`, `AZMUSIC_ERROR <message>`.

pub mod art;
pub mod ids;
pub mod library;
pub mod look;
pub mod page;
pub mod playlists;
pub mod queue;
pub mod sample;
pub mod scan;

pub mod app;
pub mod ui;

pub use app::start;
