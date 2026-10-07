//! AzPlayer: a video player on the public azul API, in the look of Windows Media Center.
//!
//! The window is the LIBRARY (the deep blue ground, "videos", the recent files as a gallery of
//! tiles, each resuming where it was left) or the STAGE: the picture - azul's `VideoWidget`
//! decoding the MP4 / MOV on its worker (VideoToolbox on Apple, Vulkan Video on x86_64 Linux /
//! Windows), in NV12, on the GPU - fills the window, and the chrome lies over it: back and
//! fullscreen at the top; the seek bar between the times, the title and the round glass
//! transport buttons (stop, from the start, back 10 s, play / pause, forward 30 s, mute, volume
//! down / up) at the bottom. The chrome hides two seconds after the pointer rests while the
//! video plays - in place, never rebuilding the window. The menu bar has the rest: Media,
//! Playback, Audio, Video, View.
//!
//! The SOUND is the same file's audio track played by azul's `AudioPlayer` (AAC through
//! Symphonia). The picture leads: a report of the video's position (four a second) moves the
//! sound to it when they stay more than 0.15 s apart ([`sync::SyncGuard`]); play, pause and
//! seek reach both.
//!
//! Fullscreen (F, F11, a double-click; Escape leaves) keeps the system awake. The keys of every
//! player: Space, Left / Right (10 s, a minute with Shift), Up / Down (volume), M (mute),
//! Backspace (the library), Mod+O.
//!
//! The data (the S3 split): `player/history.json` in the data tree (recent files, positions),
//! through the azul-storage Drive on a Thread; the videos are read where they are.
//!
//! Switches (azul-appkit): `--screen library|settings`, `--theme`, `--mode`, `--size`, `--shot`,
//! `--data-dir`, and the video files to open. On stdout, for scripts (`scripts/azplayer_e2e.py`):
//! `AZPLAYER_HISTORY <entries>`, `AZPLAYER_OPEN <path> <resume>`, `AZPLAYER_STATE <phase>
//! <position>`, `AZPLAYER_ERROR <message>`.

pub mod app;
pub mod history;
pub mod ids;
pub mod sync;
pub mod ui;

pub use app::start;
