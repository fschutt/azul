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

/// Audio playback (P7). The `AudioSink` handle is always present (codegen-
/// exposed, no feature gating); the real rodio / AVAudio output behind it is
/// on-device (the stub counts frames). The playback counterpart to
/// `MicrophoneWidget` (capture). See `audio/mod.rs`.
pub mod audio;
pub mod biometric;
/// Camera capture backend registration (v4l2 on Linux via rscam); plugs into
/// the capture_common seam. See camera/mod.rs.
pub mod camera;
/// The latest-frame mailbox the macOS capture backends hand frames through
/// (buffer reuse + condvar). Plain std, unit-tested on every host.
pub mod capture_slot;
pub mod file_picker;
/// Gamepad / game-controller input (P6 feature 6). The dispatcher pulls each
/// pad's state every frame via [`gamepad::poll`] (gilrs on desktop — pending
/// `GCController`/`InputDevice` on mobile) and parks it through
/// `azul_layout::managers::gamepad::push_gamepad_state`, which the layout
/// pass folds into the per-`App` `GamepadManager`.
pub mod gamepad;
pub mod hid;
pub mod media_keys;
pub mod geolocation;
pub mod keyring;
/// MVT tile decode + projection math for the `MapWidget` content
/// pipeline. Opt-in via the `map-tiles` Cargo feature; with the
/// feature off, the entry points return an error rather than dragging
/// in the `td` / `mvt-reader` / `proj4rs` dep tree.
pub mod map;
/// PDF (P5 AzulDoc). The `Pdf` handle is always present (so it codegen-exposes
/// with no feature-gating); the `printpdf` engine behind it is opt-in via the
/// `pdf` feature. Without it, `Pdf::from_dom` / `write_json` return empty.
pub mod pdf;
pub mod permission;
/// Platform-accelerated whole-frame scaler (Accelerate/vImage on macOS)
/// behind the `capture_common::register_frame_resampler` seam.
pub mod resample;
pub mod screencap;
/// Motion-sensor subscriptions (P6 feature 5). The dispatcher kicks the
/// platform subscription once via [`sensors::ensure_started`] (CoreMotion on
/// Apple — pending; `SensorManager` JNI on Android) and the backends park
/// each sample through `azul_layout::managers::sensors::push_sensor_reading`,
/// which the layout pass folds into the per-`App` `SensorManager`.
pub mod sensors;
/// SQLite-backed `Db` engine (P4.3). The `Db` handle is always present
/// (so it flows through the normal api.json codegen with no feature
/// gating); the bundled-SQLite `rusqlite` engine behind it is opt-in via
/// the `db-sqlite` feature. Without the feature, `Db::open` returns an
/// invalid handle and `execute`/`query` no-op (the C amalgamation isn't
/// compiled).
pub mod sqlite;
/// Video encode/decode (P7/P8). `VideoEncoder` / `VideoDecoder` handles select
/// the native codec per platform (gpu-video on desktop Linux/Windows,
/// VideoToolbox on Apple, MediaCodec on Android); the codec FFI is on-device,
/// this lands the API + selection + a stub engine. See `video_codec/mod.rs`.
pub mod video_codec;
/// WebTransport room transport (`WebTransport`) — typed media/chat/control to a
/// coordination server; replaces the removed UDP transport for azul-meet.
/// v1 = loopback stub engine;
/// real QUIC behind `webtransport-native`. See `webtransport/mod.rs`.
pub mod webtransport;
/// ZIP archives. The `Zip` handle is always present (so it codegen-exposes
/// with no feature-gating); the compressor behind it is opt-in via the `zip`
/// feature. Without it, entries still accumulate but `to_bytes` is empty.
pub mod zip;

/// Cross-subsystem capability probes ([`capability::Capability`]) — "can I use
/// this feature here, and which backend?". Non-destructive, never panic.
pub mod capability;

/// Find an APP class from a thread that has no Java frame on its stack.
///
/// `JNIEnv::find_class` resolves through the class loader of the method on top
/// of the stack. On a thread Rust created and attached — which is every thread
/// azul calls Java from, `android_main` included — there is no such method, so
/// JNI falls back to the SYSTEM class loader. That one knows `java.lang.*` and
/// nothing about the APK, so every `com/azul/...` lookup throws
/// ClassNotFoundException.
///
/// This is why the Java->Rust direction always worked (Java resolved the class
/// before calling in) while Rust->Java never has: the file picker, biometric,
/// keyring, sensors, geolocation and the soft keyboard are all on this path.
///
/// The fix is the documented one: go through the Activity's own class loader,
/// which is the APK's. `activity.getClassLoader().loadClass("com.azul.…")`.
/// Note the DOTTED name — `loadClass` takes a binary name, not the slashed
/// internal form `find_class` wants.
#[cfg(all(target_os = "android", feature = "jni"))]
pub fn find_app_class<'local>(
    env: &mut jni::JNIEnv<'local>,
    activity: &jni::objects::JObject,
    slashed_name: &str,
) -> Option<jni::objects::JClass<'local>> {
    // Fast path: on the rare thread that does have an app frame, this works
    // and costs one lookup.
    if let Ok(c) = env.find_class(slashed_name) {
        return Some(c);
    }
    let _ = env.exception_clear();

    let loader = env
        .call_method(activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
        .ok()
        .and_then(|v| v.l().ok())?;
    let dotted = slashed_name.replace('/', ".");
    let jname = env.new_string(dotted).ok()?;
    let class = env
        .call_method(
            &loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[jni::objects::JValue::Object(&jname)],
        )
        .ok()
        .and_then(|v| v.l().ok());
    if class.is_none() {
        // loadClass throws ClassNotFoundException for a genuinely absent
        // optional helper; leaving it pending would abort the process on the
        // next JNI call.
        let _ = env.exception_clear();
    }
    class.map(Into::into)
}
