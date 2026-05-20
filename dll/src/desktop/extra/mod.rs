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
/// SQLite-backed `Db` engine (P4.3). The `Db` handle is always present
/// (so it flows through the normal api.json codegen with no feature
/// gating); the bundled-SQLite `rusqlite` engine behind it is opt-in via
/// the `db-sqlite` feature. Without the feature, `Db::open` returns an
/// invalid handle and `execute`/`query` no-op (the C amalgamation isn't
/// compiled).
pub mod sqlite;
/// MVT tile decode + projection math for the `MapWidget` content
/// pipeline. Opt-in via the `map-tiles` Cargo feature; with the
/// feature off, the entry points return an error rather than dragging
/// in the `td` / `mvt-reader` / `proj4rs` dep tree.
pub mod map;
/// PDF export (P5 AzulDoc). The export API is always present (so it codegen-
/// exposes with no feature-gating); the `printpdf` engine behind it is opt-in
/// via the `pdf` feature. Without it, `export_to_pdf` returns `false`.
pub mod pdf;
pub mod permission;
/// Motion-sensor subscriptions (P6 feature 5). The dispatcher kicks the
/// platform subscription once via [`sensors::ensure_started`] (CoreMotion on
/// Apple — pending; `SensorManager` JNI on Android) and the backends park
/// each sample through `azul_layout::managers::sensors::push_sensor_reading`,
/// which the layout pass folds into the per-`App` `SensorManager`.
pub mod sensors;
