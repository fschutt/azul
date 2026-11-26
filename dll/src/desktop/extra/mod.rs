//! `azul_dll::desktop::extra` — platform integrations for features that
//! aren't part of the layout core.
//!
//! Every camera / screen-capture / biometric /
//! sensor / map / PDF / SQLite / location / file-picker integration lives
//! here so that `azul-core`, `azul-css`, and `azul-layout` stay
//! dependency-light (no `objc` / `WinRT` / `pipewire` / `libsql` etc. in
//! the layout closure).
//!
//! Each submodule re-exports the public surface so callers reach into a
//! flat namespace, e.g. `azul_dll::desktop::extra::permission::apply_diff_events`.

/// Audio playback (P7). The `AudioSink` handle is always present (codegen-
/// exposed, no feature gating); it is open only where an output device
/// opened (ALSA / WASAPI / AAudio / AVAudioEngine), else closed with a reason.
/// The playback counterpart to `MicrophoneWidget` (capture). See
/// `audio/mod.rs`.
pub mod audio;
/// `AVAudioSession` on iOS: the category a sink or a microphone needs, set before its engine is built.
pub mod audio_session;
/// Sign-in sessions (OAuth 2.0 for native apps): `ASWebAuthenticationSession` on Apple, the
/// system browser with a loopback listener on the desktops, a Custom Tab on Android, a fake
/// in headless runs. See `auth_session/mod.rs`.
pub mod auth_session;
pub mod biometric;
/// The bytes of a media file read where they are - a local file, an HTTP(S)
/// URL by range requests (a download a window ahead of its readers, shared by
/// a video's picture and sound), bytes in memory - so a player never needs
/// the whole file before its first frame. See `byte_source.rs`.
pub(crate) mod byte_source;
/// Camera capture backend registration (v4l2 on Linux via rscam); plugs into
/// the capture_common seam. See camera/mod.rs.
pub mod camera;
/// The latest-frame mailbox the macOS capture backends hand frames through
/// (buffer reuse + condvar). Plain std, unit-tested on every host.
pub mod capture_slot;
/// What the platform services read from the environment (typed; the one place that names the
/// variables).
pub mod extra_settings;
pub mod file_picker;
/// Gamepad / game-controller input (P6 feature 6). The dispatcher pulls each
/// pad's state every frame via [`gamepad::poll`] (gilrs on desktop — pending
/// `GCController`/`InputDevice` on mobile) and parks it through
/// `azul_layout::managers::gamepad::push_gamepad_state`, which the layout
/// pass folds into the per-`App` `GamepadManager`.
pub mod gamepad;
pub mod geolocation;
pub mod haptics;
pub mod hid;
/// What the app's own `Info.plist` says (the purpose strings the protected APIs need), so a request
/// that would stop the process is refused with a diagnostic instead.
pub mod info_plist;
/// Peer-to-peer QUIC connections (`IrohEndpoint`) and forwarder selection; the engine needs the `iroh` feature.
pub mod iroh;
pub mod keyring;
/// MVT tile decode + projection math for the `MapWidget` content
/// pipeline. Opt-in via the `map-tiles` Cargo feature; with the
/// feature off, the entry points return an error rather than dragging
/// in the `td` / `mvt-reader` / `proj4rs` dep tree.
pub mod map;
pub mod media_keys;
pub mod natural_scroll;
/// PDF (P5 AzulDoc). The `Pdf` handle is always present (so it codegen-exposes
/// with no feature-gating); the `printpdf` engine behind it is opt-in via the
/// `pdf` feature. Without it, `Pdf::from_dom` / `write_json` return empty.
pub mod pdf;
pub mod permission;
/// Platform-accelerated whole-frame scaler (Accelerate/vImage on macOS)
/// behind the `capture_common::register_frame_resampler` seam.
pub mod resample;
pub mod screencap;
/// The device's sensors and states, every reading of the device in one place.
///
/// - Motion-sensor subscriptions (P6 feature 5). The dispatcher kicks the
///   platform subscription once via [`sensors::ensure_started`] (CoreMotion on
///   Apple — pending; `SensorManager` JNI on Android) and the backends park
///   each sample through `azul_layout::managers::sensors::push_sensor_reading`,
///   which the layout pass folds into the per-`App` `SensorManager`.
/// - The device-state readings for background work: the power state
///   ([`sensors::power`]), the battery and the device's temperature ([`sensors::battery`]) and
///   the network ([`sensors::network`]), each a synchronous query
///   of a cached or cheap reading; a fixed reading (or a test's switch file) in headless runs.
pub mod sensors;
/// The desktop's state, polled: the app in front, the pointer on the desktop, the lock and the display's
/// sleep (`DesktopState::query`): the readings of a recorder or an app that reacts to the desktop, kept
/// apart from [`sensors`] because they say more about the person than the device does.
pub mod desktop_state;
/// The user's session: the screen locked or unlocked, the machine about to
/// sleep or awake (`SessionChange` events), from the platform's observers.
pub mod session;
/// Speech on the platform's engines: text said aloud (AVSpeechSynthesizer, SAPI,
/// speech-dispatcher) and what the user says heard (SFSpeechRecognizer, WinRT); logged, not
/// spoken, in a headless run. See `speech/mod.rs`.
pub mod speech;
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
/// Bringing a window to the front, per platform (9h-i-a-ii).
pub mod window_activation;
/// ZIP archives. The `Zip` handle is always present (so it codegen-exposes
/// with no feature-gating); the compressor behind it is opt-in via the `zip`
/// feature. Without it, entries still accumulate but `to_bytes` is empty.
pub mod zip;

/// Cross-subsystem capability probes ([`capability::Capability`]) — "can I use
/// this feature here, and which backend?". Non-destructive, never panic.
pub mod capability;

/// A JNI local-reference frame, popped when it drops.
///
/// Every `JObject`, `JString` and `JClass` a call hands back is a LOCAL reference, and a local
/// reference lives until the thread returns to Java or detaches. `android_main` is a native thread that
/// is attached for the whole run and never returns to Java, so everything a helper allocates on it
/// stays allocated: the JNI tips say "if you attach a native thread with AttachCurrentThread, the code
/// you are running will never automatically free local references until the thread detaches", and
/// before Android 8.0 the table is capped (512 on Android 7: the VM aborts the process). A haptic tap,
/// a keyboard that shows and hides, a paste, a permission probe: each leaked a handful of references
/// (`find_app_class` alone: three), on the one thread that never lets go. Push a frame at the top of
/// every helper that runs on it; the frame is dropped BEFORE the attach guard, so the references are
/// released before the thread could detach.
///
/// Values that must outlive the frame (an owned `String`, a `bool`) are Rust values: copy them out
/// first. A `JObject` may not leave the frame.
#[cfg(all(target_os = "android", feature = "jni"))]
pub struct LocalFrame {
    env: *mut jni::sys::JNIEnv,
    pushed: bool,
}

#[cfg(all(target_os = "android", feature = "jni"))]
impl LocalFrame {
    /// How many references a helper may hold at once (a class loader, a name, a class, a few results).
    pub const DEFAULT_CAPACITY: i32 = 32;

    /// Push a frame of `capacity` references on `env`'s thread. When the VM cannot (out of memory) the
    /// guard is inert and the helper carries on as it did without one.
    #[must_use]
    pub fn push(env: &jni::JNIEnv<'_>, capacity: i32) -> Self {
        // An exception a careless caller left pending (`.ok()?` on a failed call does) makes EVERY later
        // JNI call on this thread fail or abort: "you must not call most JNI functions while an exception
        // is pending". It is cleared on the way in as well as on the way out.
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
        Self {
            env: env.get_raw(),
            pushed: env.push_local_frame(capacity).is_ok(),
        }
    }
}

#[cfg(all(target_os = "android", feature = "jni"))]
impl Drop for LocalFrame {
    fn drop(&mut self) {
        if !self.pushed {
            return;
        }
        // SAFETY: the pointer is the environment of the thread this guard lives on (a guard is never
        // sent), and the frame it pushed is the innermost one: guards drop in reverse order.
        if let Ok(env) = unsafe { jni::JNIEnv::from_raw(self.env) } {
            // A failed call inside the scope may have left an exception pending: clear it, so it cannot
            // poison the next helper that runs on this thread.
            if env.exception_check().unwrap_or(false) {
                let _ = env.exception_clear();
            }
            let _ = unsafe { env.pop_local_frame(&jni::objects::JObject::null()) };
        }
    }
}

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
