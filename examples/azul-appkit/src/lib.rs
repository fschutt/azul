//! azul-appkit: the skeleton the Azlin apps share (build ledger F2, F6, F7).
//!
//! - [`args`]: the switches every app understands (`--screen`, `--size`,
//!   `--theme`, `--mode`, `--shot`, `--sample`, `--data-dir`).
//! - [`data`]: the per-user data layout - one folder per app under the data
//!   root, keyed as the user's S3 bucket will be.
//! - [`settings`]: `<app>/settings.json` (app theme, mode, the app's values).
//! - [`shortcuts`]: the keyboard-shortcut table, `Mod` = Cmd / Ctrl.
//! - [`about`]: the About facts.
//! - [`files`]: file jobs (put / get / get-all / delete) on an azul-storage drive.
//! - `ui` (feature `azul`): the settings page on azul's `ShellSettingsLayout`
//!   (Appearance, Data, Shortcuts, About, plus the app's own sections), the
//!   window's title row, the window options (`NoTitle`, `--size`), the app
//!   config (`--theme` / `--mode` over the settings file), the `--shot`
//!   screenshot timer, and [`files`] jobs on an azul `Thread`.
//!
//! Everything but `ui` is plain Rust and tested without a window:
//! `cargo test -p azul-appkit`.

pub mod about;
pub mod args;
pub mod data;
pub mod files;
pub mod settings;
pub mod shortcuts;

#[cfg(feature = "azul")]
pub mod ui;

pub use about::AboutInfo;
pub use args::{AppArgs, AppSpec, ModePref, Theme};
pub use files::{FileJob, FileOutcome};
pub use settings::AppSettings;
pub use shortcuts::Shortcut;
