//! `azul_dll::desktop::extra` — platform integrations for features that
//! aren't part of the layout core.
//!
//! Per `SUPER_PLAN_2.md` §0.5, every camera / screen-capture / biometric /
//! sensor / map / PDF / SQLite / location / file-picker integration lives
//! here so that `azul-core`, `azul-css`, and `azul-layout` stay
//! dependency-light (no `objc` / `WinRT` / `pipewire` / `libsql` etc. in
//! the layout closure).
//!
//! Each submodule re-exports the public surface so callers reach into a
//! flat namespace, e.g. `azul_dll::desktop::extra::permission::apply_diff_events`.

pub mod biometric;
pub mod file_picker;
pub mod geolocation;
pub mod keyring;
/// SQLite-backed `Db` engine (P4.3). Opt-in via the `db-sqlite` Cargo
/// feature; with the feature off, the `Db` API isn't built (the bundled
/// SQLite C amalgamation isn't compiled).
#[cfg(feature = "db-sqlite")]
pub mod sqlite;
/// MVT tile decode + projection math for the `MapWidget` content
/// pipeline. Opt-in via the `map-tiles` Cargo feature; with the
/// feature off, the entry points return an error rather than dragging
/// in the `td` / `mvt-reader` / `proj4rs` dep tree.
pub mod map;
pub mod permission;
