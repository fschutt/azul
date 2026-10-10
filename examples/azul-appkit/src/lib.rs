//! azul-appkit: the skeleton the Azlin apps share (build ledger F2, F6, F7).
//!
//! - [`args`]: the switches every app understands (`--screen`, `--size`,
//!   `--theme`, `--mode`, `--shot`, `--sample`, `--data-dir`).
//! - [`css`]: style sheets from outside the app (a book's, a mail's) read
//!   rule by rule, for the app's own policy of what to keep.
//! - [`csv`]: the one CSV reader (a header and its rows; the separator
//!   guessed from the header line).
//! - [`data`]: the per-user data layout - one folder per app under the data
//!   root, keyed as the user's S3 bucket will be.
//! - [`settings`]: `<app>/settings.json` (app theme, mode, the app's values).
//! - [`azlin_config`]: `~/.azlin/config.json`, the one config every Azlin app
//!   shares (`currentTheme`, `mode`): read at the start, written when the look
//!   changes, so a theme chosen in one app is every app's. Its `endpoints`
//!   section says where the Azlin services are (a profile's built-in
//!   addresses, overridden by the file, the environment, a flag; with where
//!   each value came from).
//! - [`shared_endpoint`]: a service's address from the shared config's
//!   `endpoints` section (the meeting server, the tile server, S3), for an
//!   app to weigh under its own switch and variable.
//! - [`shortcuts`]: the keyboard-shortcut table, `Mod` = Cmd / Ctrl.
//! - [`about`]: the About facts.
//! - [`client_health`]: this device's client health, 0 to 100, for how much background work
//!   it takes on - computed locally from azul's device-state readings (power, battery,
//!   network); only the number may ever travel, never its parts.
//! - [`files`]: file jobs (put / get / get-all / delete) on an azul-storage drive.
//! - [`history`]: undo / redo of whole-state snapshots ([`UndoHistory`]).
//! - [`migrate`]: the one-time move of an app's folder from the data folders
//!   older builds used (`azul/`, `Azul/`, `AzNotes/`) into the data root.
//! - [`qr`]: QR codes for paper (byte mode, level M, versions 1 to 10): the
//!   recovery code on AzDrive's emergency kit, a trusted contact's share.
//! - [`phrase`]: words of an app's Fluent resources kept as a key and its arguments until
//!   they are shown ([`phrase::Phrase`], [`phrase::Text`]).
//! - [`l10n_check`]: the checks every app's localization test runs (every key in English and
//!   German, both resources parse).
//! - `l10n` (feature `look`, part of `azul`): an app's words through azul's localization -
//!   its resources registered with the engine, keys in the DOM (`AzString::tr`), the text that
//!   is no DOM text node in the window's language.
//! - [`options`]: the settings page's model - Outlook 2010's Options dialog
//!   (the categories, the header line over each, the ids, what Cancel puts
//!   back).
//! - `pieces` (feature `look`, part of `azul`): the small DOM pieces every
//!   app's screens are built from (text, block, flex column / row, buttons).
//! - `look` (feature `look`, part of `azul`): the settings page's look -
//!   Outlook 2010's Options dialog in pieces (the category list, the header
//!   line, banded sections, rows, OK / Cancel) - for the kit's page and for an
//!   app that draws its settings itself and links azul its own way (AzMeet).
//! - `backstage` (feature `azul`): the right side of the File tab in the
//!   Outlook 2010 look (a page's title, cards, command rows of a button, a
//!   heading and what it does, two columns, sections, facts).
//! - `ribbon` (feature `azul`): the office apps' ribbon buttons, columns,
//!   rows and groups (one builder; the app's command type implements
//!   `RibbonCommand`).
//! - `ui` (feature `azul`): the settings page in the shape of Outlook 2010's
//!   Options dialog (the app's own categories, then General, Data, Shortcuts,
//!   About; a header line, banded sections, OK / Cancel), the
//!   window's title row, the window options (`NoTitle`, `--size`), the app
//!   config (`--theme` / `--mode` over the settings file; Haiku's icons, the
//!   flora theme's, from azul-icons-haiku), the `--shot`
//!   screenshot timer, and [`files`] jobs on an azul `Thread`.
//!
//! Everything but `pieces` and `ui` is plain Rust and tested without a window:
//! `cargo test -p azul-appkit`.

pub mod about;
pub mod args;
pub mod azlin_config;
pub mod client_health;
pub mod css;
pub mod csv;
pub mod data;
pub mod files;
pub mod find;
pub mod history;
pub mod l10n_check;
pub mod migrate;
pub mod oauth_clients;
pub mod options;
pub mod phrase;
pub mod qr;
pub mod settings;
pub mod shared_endpoint;
pub mod shortcuts;
#[cfg(test)]
mod l10n_tests;
#[cfg(all(test, feature = "azul"))]
mod l10n_switch_tests;

#[cfg(feature = "azul")]
pub mod backstage;
#[cfg(feature = "look")]
pub mod l10n;
#[cfg(feature = "look")]
pub mod look;
#[cfg(feature = "look")]
pub mod pieces;
#[cfg(feature = "azul")]
pub mod ribbon;
#[cfg(feature = "azul")]
pub mod ui;

pub use about::AboutInfo;
pub use args::{AppArgs, AppSpec, ModePref, Theme};
pub use azlin_config::AzlinConfig;
pub use files::{FileJob, FileOutcome};
pub use history::UndoHistory;
pub use settings::AppSettings;
pub use shortcuts::Shortcut;
