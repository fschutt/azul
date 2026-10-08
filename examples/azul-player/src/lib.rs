//! AzPlayer: a media center on the public azul API, in the look of Windows Media Center (7).
//!
//! THE START STRIP is the first screen: the categories stacked down the window - extras,
//! pictures + videos, music, movies, tv, tasks - the focused one on the middle row with its items
//! beside it (music library, play all, radio, search; picture library, play favorites, video
//! library; movie library, open a file, open an address; recorded tv; settings, media only,
//! refresh, about, close).
//! Arrow keys, the wheel and the pointer move through it, Enter opens; an item that cannot work
//! here says why. Every library is a page: its big lower-case title, its views (albums · artists
//! · genres · songs; folders · date taken · play slide show; ...), a gallery of tiles - covers,
//! thumbnails, initials - that runs to the right. Back (Backspace, Escape, the back button, the
//! mouse's back button) works everywhere; the Media Center orb goes home.
//!
//! WHAT PLAYS: music from the music folder (azul's `AudioPlayer`, gapless, now playing with the
//! cover, the seek bar and what comes next); pictures in a viewer and a slide show (Ken Burns'
//! pan and zoom, a cross-fade from one to the next); videos and movies (MP4 / MOV with H.264:
//! azul's `VideoWidget` for the picture, the same `AudioPlayer` for the sound), from a file or a
//! web address (read by range requests while it plays, picture and sound through one download).
//! A video opens
//! BEHIND THE CURTAIN (`curtain.rs`): its first picture and its first sound are made ready out of
//! sight, then the menus fade to black - text, icons, then the blue and its light - and the
//! picture fades in as picture and sound start together. The transport (bottom right) shows when
//! the pointer moves and hides while it rests.
//!
//! THE LOOK (`look.rs`): Media Center's deep blue lit by flora's light shafts, drifting slowly
//! when the page or the category changes; the foreground on springs; the focus a soft blue-white
//! glow and a slight scale (the engine's focus ring is off: the app draws its own focus
//! everywhere). Only transforms and opacity animate - no animation frame lays the window out.
//!
//! OFF THE UI THREAD (`scan.rs`): the library scans (files, a song's tags, a video's length), the
//! pictures (thumbnails, covers, the viewer's copies), the files of the data tree; the decoding of
//! sound and picture runs on the player's and the widget's own threads.
//!
//! The data (the S3 split): `player/history.json` (recent files, positions) and
//! `player/library.json` (what the last scans found) in the data tree, through the azul-storage
//! Drive on a Thread; the media are read where they are.
//!
//! Switches (`args.rs`; every setting is a switch, no environment variable): appkit's `--screen
//! start|music|pictures|videos|movies|tv|recent|now-playing|settings`, `--theme`, `--mode`,
//! `--size`, `--shot`, `--data-dir`, and AzPlayer's `--music-dir`, `--pictures-dir`,
//! `--videos-dir`, `--tv-dir`; bare arguments are videos to open. On stdout, for scripts
//! (`scripts/azplayer_e2e.py`): `AZPLAYER_HISTORY <n>`, `AZPLAYER_LIBRARY cached <music>
//! <pictures> <videos> <tv>`, `AZPLAYER_SCAN <library> <n> <ready|missing>`, `AZPLAYER_PAGE
//! <page>`, `AZPLAYER_ACTION <action>`, `AZPLAYER_VIEW <view>`, `AZPLAYER_GROUP <title>`,
//! `AZPLAYER_NOTICE <sentence>`, `AZPLAYER_OPEN <path> <resume>`, `AZPLAYER_CURTAIN
//! <preroll|fade-out|play|open>`, `AZPLAYER_PREROLL <picture|sound> <...>`, `AZPLAYER_AUDIO
//! <ready|none> <buffered>`, `AZPLAYER_STATE <phase> <position>`, `AZPLAYER_CLOSE <position>`,
//! `AZPLAYER_MUSIC <play title|playing|paused|stopped|finished>`, `AZPLAYER_PICTURE <index>
//! <still|slideshow>`, `AZPLAYER_SLIDESHOW <playing|paused>`, `AZPLAYER_SEARCH <found>`,
//! `AZPLAYER_FULLSCREEN <on|off>`, `AZPLAYER_FOCUS <category / item | tile n column c>`,
//! `AZPLAYER_ADDRESS <url>`, `AZPLAYER_QUIT`, `AZPLAYER_ERROR <message>`.

pub mod app;
pub mod args;
pub mod curtain;
pub mod dialog;
pub mod gallery;
pub mod history;
pub mod ids;
pub mod library;
pub mod look;
pub mod media;
pub mod nav;
pub mod options;
pub mod overlay;
pub mod pages;
pub mod scan;
pub mod settings;
pub mod strip;
pub mod sync;
pub mod ui;

pub use app::start;
