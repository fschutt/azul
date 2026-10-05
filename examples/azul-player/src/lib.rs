//! AzPlayer: a video player on the public azul API.
//!
//! The window is azul's S7 `MediaShell` in its player variant: the stage (the picture: azul's
//! `VideoWidget` decoding the MP4 / MOV on its worker - VideoToolbox on Apple, Vulkan Video on
//! x86_64 Linux / Windows) or the library (the recent files, each resuming where it was left), and
//! the controls bar under it - azul's `MediaControls` (with the -15 s / +30 s skips and the volume)
//! and `SeekBar` - hidden two seconds after the pointer rests while the video plays.
//!
//! The SOUND is the same file's audio track played by azul's `AudioPlayer` (AAC through
//! Symphonia). The picture leads: every report of the video's position (four a second) moves the
//! sound to it when they are more than 0.15 s apart ([`sync::audio_correction`]); play, pause and
//! seek reach both.
//!
//! Fullscreen (F, F11, a double-click; Escape leaves) keeps the system awake. The keys of every
//! player: Space, Left / Right (10 s, a minute with Shift), Up / Down (volume), M (mute), Mod+O.
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
