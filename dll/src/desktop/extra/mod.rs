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
/// PDF (P5 AzulDoc). The `Pdf` handle is always present (so it codegen-exposes
/// with no feature-gating); the `printpdf` engine behind it is opt-in via the
/// `pdf` feature. Without it, `Pdf::from_dom` / `write_json` return empty.
pub mod pdf;
/// Audio playback (P7). The `AudioSink` handle is always present (codegen-
/// exposed, no feature gating); the real rodio / AVAudio output behind it is
/// on-device (the stub counts frames). The playback counterpart to
/// `MicrophoneWidget` (capture). See `audio/mod.rs`.
pub mod audio;
/// Camera capture backend registration (v4l2 on Linux via rscam); plugs into
/// the capture_common seam. See camera/mod.rs.
pub mod camera;
/// UDP transport (P8). The `Udp` handle wraps a `std::net::UdpSocket` (no
/// feature gate - `std::net` is always present, real on every target). The
/// fault-tolerant packet-sharing primitive for azul-meet. See `udp/mod.rs`.
pub mod udp;
/// Video encode/decode (P7/P8). `VideoEncoder` / `VideoDecoder` handles select
/// the native codec per platform (gpu-video on desktop Linux/Windows,
/// VideoToolbox on Apple, MediaCodec on Android); the codec FFI is on-device,
/// this lands the API + selection + a stub engine. See `video_codec/mod.rs`.
pub mod video_codec;
pub mod permission;
/// Motion-sensor subscriptions (P6 feature 5). The dispatcher kicks the
/// platform subscription once via [`sensors::ensure_started`] (CoreMotion on
/// Apple — pending; `SensorManager` JNI on Android) and the backends park
/// each sample through `azul_layout::managers::sensors::push_sensor_reading`,
/// which the layout pass folds into the per-`App` `SensorManager`.
pub mod sensors;
/// Gamepad / game-controller input (P6 feature 6). The dispatcher pulls each
/// pad's state every frame via [`gamepad::poll`] (gilrs on desktop — pending
/// `GCController`/`InputDevice` on mobile) and parks it through
/// `azul_layout::managers::gamepad::push_gamepad_state`, which the layout
/// pass folds into the per-`App` `GamepadManager`.
pub mod gamepad;

/// Cross-subsystem capability probes ([`capability::Capability`]) — "can I use
/// this feature here, and which backend?". Non-destructive, never panic.
pub mod capability;
