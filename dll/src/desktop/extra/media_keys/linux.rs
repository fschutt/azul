//! Linux media keys via MPRIS over D-Bus.
//!
//! # Why this is needed when 9h-i already maps the keysyms
//!
//! 9h-i added `XF86AudioPlay` and friends to the X11/Wayland keysym table, and
//! that is the whole answer WHEN NOTHING GRABBED THEM. Every mainstream desktop
//! does grab them: GNOME, KDE and the rest bind the media row globally and
//! route it to whatever media players are registered, so the keysym never
//! reaches the focused window at all.
//!
//! The registration mechanism is MPRIS: an app claims
//! `org.mpris.MediaPlayer2.<something>` on the session bus and exports a
//! `Player` interface, and the desktop calls `Play`/`Pause`/`Next` on it.
//!
//! # It is OPT-IN, and that is a product decision not a technical one
//!
//! Registering has a VISIBLE consequence: the app shows up in the desktop's
//! media controls as a player. Correct for a music app, wrong for a text
//! editor, and there is no engine-side signal that distinguishes them - so
//! `AppConfig::expose_system_media_controls` says which, and defaults to off.
//!
//! # What the method calls become
//!
//! Ordinary `VirtualKeyCode` presses, because that is the contract every other
//! media-key producer follows: the Win32 `WM_APPCOMMAND` arm and the keysym
//! table both deliver `PlayPause` as a normal key. An app binding it works
//! unchanged whether the key arrived raw or through D-Bus.
//!
//! # The properties answer from what the APP published
//!
//! `PlaybackStatus`, `Metadata` and `Position` used to be hardcoded to a
//! stopped player with no track, because azul has no playback state machine and
//! never will have one that sees an app playing audio through `rodio`, a system
//! framework or the network. So the app publishes instead
//! (`CallbackInfo::set_now_playing`), and these read what it published.
//!
//! An app that publishes nothing still gets the stub, and that is load-bearing
//! rather than lazy: `PlaybackStatus` is REQUIRED by the spec and read by every
//! desktop, and omitting it makes some of them treat the player as broken and
//! hide it - taking the transport buttons with it. The stub is what makes the
//! KEYS work.
//!
//! # Announcing versus answering
//!
//! Two mechanisms, and the split is the spec's, not a shortcut. The getters
//! here ANSWER `org.freedesktop.DBus.Properties.Get` at any time and always see
//! the newest value. `publish` additionally ANNOUNCES a `PropertiesChanged`
//! signal, because most clients cache and never poll.
//!
//! `Position` is announced by NEITHER, deliberately: the spec forbids putting
//! it in `PropertiesChanged` because it advances continuously - clients
//! extrapolate it from `PlaybackStatus` and `Rate`, and read the property when
//! they need a precise value. That is why `MediaSessionManager` does not mark
//! itself dirty for a position-only change.

use std::sync::atomic::{AtomicU64, Ordering};

use azul_core::{
    media_session::{MediaPlaybackState, NowPlayingInfo},
    window::VirtualKeyCode,
};
use azul_layout::managers::media_keys::push_media_key;

/// The object path both interfaces are served at. Fixed by the MPRIS spec:
/// clients look here and nowhere else.
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";

/// What the app last published, or `None` if it never has.
static SESSION: std::sync::Mutex<Option<NowPlayingInfo>> = std::sync::Mutex::new(None);

/// The live connection, kept so `publish` can emit on it.
///
/// This is also what keeps the bus name claimed: dropping the connection
/// releases the name and the desktop forgets the player, so it must outlive
/// the thread's scope.
static CONN: std::sync::OnceLock<zbus::blocking::Connection> = std::sync::OnceLock::new();

/// The window that registered the session, for `Raise` (9h-i-a-ii).
///
/// MPRIS is per-PROCESS but a raise has to name a WINDOW, and an app can have
/// several. The one that registered is the honest answer: it is the window the
/// desktop's media widget is showing.
static RAISE_TARGET: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Bumped only when the TRACK changes, never on a pause or a seek - see
/// `NowPlayingInfo::is_different_track` for why that distinction matters to a
/// desktop's progress bar.
static TRACK_SERIAL: AtomicU64 = AtomicU64::new(0);

/// A copy of what the app published, for a property getter to answer with.
fn session() -> Option<NowPlayingInfo> {
    SESSION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// `Metadata`, built from the published session.
///
/// EMPTY when there is no track, which is what the spec asks for and what makes
/// a desktop widget show its own idle state instead of a blank title.
fn metadata_map() -> std::collections::HashMap<String, zbus::zvariant::OwnedValue> {
    use zbus::zvariant::{ObjectPath, OwnedValue, Value};

    let mut map = std::collections::HashMap::new();
    let Some(info) = session() else {
        return map;
    };
    let has_track = !info.title.as_str().is_empty()
        || !info.artist.as_str().is_empty()
        || !info.album.as_str().is_empty()
        || info.duration_ms != 0;
    if !has_track {
        return map;
    }

    let mut put = |key: &str, value: Value<'_>| {
        if let Ok(v) = OwnedValue::try_from(value) {
            map.insert(key.to_string(), v);
        }
    };

    // REQUIRED whenever the map is non-empty, and it must be an object PATH
    // (signature `o`), not a string - a client that deserialises it strictly
    // drops the whole map otherwise. Built from a counter rather than from the
    // title so that two identical tracks played in a row are still two tracks.
    let serial = TRACK_SERIAL.load(Ordering::Relaxed);
    if let Ok(path) = ObjectPath::try_from(format!("{OBJECT_PATH}/azul/track/{serial}")) {
        put("mpris:trackid", Value::from(path));
    }
    if info.duration_ms != 0 {
        // MICROSECONDS. Milliseconds here is the classic MPRIS bug: every
        // duration comes out 1000x short and the progress bar finishes
        // instantly.
        put("mpris:length", Value::from(info.duration_us()));
    }
    if !info.artwork_url.as_str().is_empty() {
        put("mpris:artUrl", Value::from(info.artwork_url.as_str().to_string()));
    }
    if !info.title.as_str().is_empty() {
        put("xesam:title", Value::from(info.title.as_str().to_string()));
    }
    if !info.artist.as_str().is_empty() {
        // An ARRAY of strings (`as`), not a string: the type is fixed by the
        // spec even for the one-artist case that every widget joins back into
        // a single line.
        put(
            "xesam:artist",
            Value::from(vec![info.artist.as_str().to_string()]),
        );
    }
    if !info.album.as_str().is_empty() {
        put("xesam:album", Value::from(info.album.as_str().to_string()));
    }
    map
}

/// The MPRIS `Player` interface, reduced to the transport controls.
///
/// Only the methods a media KEY can produce are implemented. `Seek`,
/// `SetPosition` and `OpenUri` are absent: no key produces them, and answering
/// them would imply a position azul does not have.
struct MprisPlayer;

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl MprisPlayer {
    fn play(&self) {
        push_media_key(VirtualKeyCode::PlayPause);
    }

    fn pause(&self) {
        push_media_key(VirtualKeyCode::PlayPause);
    }

    /// The one the media KEY actually sends: a keyboard's play button is a
    /// toggle, and desktops map it here rather than to `Play` or `Pause`.
    #[zbus(name = "PlayPause")]
    fn play_pause(&self) {
        push_media_key(VirtualKeyCode::PlayPause);
    }

    fn stop(&self) {
        push_media_key(VirtualKeyCode::MediaStop);
    }

    fn next(&self) {
        push_media_key(VirtualKeyCode::NextTrack);
    }

    fn previous(&self) {
        push_media_key(VirtualKeyCode::PrevTrack);
    }

    /// What the app published, or `Stopped` if it never published anything.
    ///
    /// REQUIRED by the spec and read by every desktop; omitting it makes some
    /// of them treat the player as broken and hide it, taking the transport
    /// buttons with it - so even the never-published case must answer.
    ///
    /// THE STRINGS ARE THE WIRE FORMAT and are case-sensitive: a client
    /// comparing against `"Playing"` sees anything else as unknown and shows a
    /// play button for a track that is already playing.
    #[zbus(property)]
    fn playback_status(&self) -> String {
        match session().map(|s| s.state) {
            Some(MediaPlaybackState::Playing) => "Playing".to_string(),
            Some(MediaPlaybackState::Paused) => "Paused".to_string(),
            _ => "Stopped".to_string(),
        }
    }

    /// The published track, or an empty map when there is none.
    #[zbus(property)]
    fn metadata(&self) -> std::collections::HashMap<String, zbus::zvariant::OwnedValue> {
        metadata_map()
    }

    /// How far into the track, in MICROSECONDS.
    ///
    /// Answered on demand and never announced - see the module docs. A client
    /// that wants a smooth progress bar extrapolates from here using `Rate`.
    #[zbus(property)]
    fn position(&self) -> i64 {
        session().map(|s| s.position_us()).unwrap_or(0)
    }

    /// Playback speed, and its bounds. All `1.0`: azul does not model a rate,
    /// and the spec requires the three properties to exist regardless.
    ///
    /// A missing `MinimumRate`/`MaximumRate` is not harmless - a client that
    /// reads them to size a speed control treats the absence as `0.0` and
    /// offers a control that stops playback.
    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }

    /// The `Can*` flags gate the desktop's BUTTONS. All true, because the app
    /// receives every one of these as a key and decides for itself - reporting
    /// false would grey out a button the app may well handle.
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }

    /// FALSE, unlike the others: seeking needs a position, and azul has none.
    /// Claiming it would put a scrubber in the desktop UI that does nothing.
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

/// The root `org.mpris.MediaPlayer2` interface.
///
/// Required alongside `Player`: a desktop that cannot read `Identity` treats
/// the registration as malformed and ignores the player entirely.
struct MprisRoot {
    identity: String,
}

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl MprisRoot {
    /// The desktop asking to focus the app from its media widget.
    ///
    /// Parked rather than performed: this runs on the D-Bus thread, and
    /// activating a window is a call that belongs on the event loop - which is
    /// also the only place that knows whether the window still exists. The
    /// loop takes it on its next pass (9h-i-a-ii).
    fn raise(&self) {
        let target = RAISE_TARGET.load(Ordering::Relaxed);
        azul_layout::managers::window_activation::request_raise(target);
    }

    /// NOT wired to a quit. A media widget's close button terminating the app
    /// would be a surprise; `CanQuit` reports false so no desktop offers it.
    fn quit(&self) {}

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }

    /// TRUE now that `Raise` does something. A desktop greys out its "open the
    /// player" affordance when this is false, so leaving it false while the
    /// method worked would hide the feature.
    ///
    /// It stays true even where the platform will decline - a Wayland session
    /// cannot raise on request - because this advertises what the APP
    /// supports, and the refusal is the compositor's answer rather than a
    /// missing capability.
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> String {
        self.identity.clone()
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        Vec::new()
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Claim the bus name and serve the interfaces, on a thread of its own.
///
/// Idempotent and quiet: no session bus (a headless build, a CI container) is
/// the normal case, not an error.
pub fn start(window_id: u64) {
    if !azul_layout::window::expose_system_media_controls() {
        return;
    }
    // Recorded before the OnceLock guard, so a second window calling this does
    // not silently leave `Raise` pointing at a window that has since closed.
    RAISE_TARGET.store(window_id, std::sync::atomic::Ordering::Relaxed);
    static STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }

    std::thread::Builder::new()
        .name("azul-mpris".into())
        .spawn(|| {
            // The bus name must be unique per process: two instances of the
            // same app would otherwise fight over one name and the second
            // would fail to register, silently losing its media keys.
            let name = format!("org.mpris.MediaPlayer2.azul.instance{}", std::process::id());
            let identity = std::env::args()
                .next()
                .and_then(|p| {
                    std::path::Path::new(&p)
                        .file_name()
                        .map(|f| f.to_string_lossy().into_owned())
                })
                .unwrap_or_else(|| "azul".to_string());

            let built = zbus::blocking::connection::Builder::session()
                .and_then(|b| b.name(name.as_str()))
                .and_then(|b| b.serve_at("/org/mpris/MediaPlayer2", MprisPlayer))
                .and_then(|b| b.serve_at("/org/mpris/MediaPlayer2", MprisRoot { identity }))
                .and_then(zbus::blocking::connection::Builder::build);

            match built {
                Ok(conn) => {
                    crate::plog_info!("[media-keys] MPRIS registered as {}", name);
                    // The connection must OUTLIVE this scope or the name is
                    // released the moment it drops and the desktop forgets the
                    // player. Parking it in the static is what keeps it alive
                    // AND what gives `publish` something to emit on; the
                    // interfaces are served from zbus's own executor.
                    let _ = CONN.set(conn);
                    // An app may have published BEFORE the bus was up - the
                    // connection takes a moment and a player sets its track at
                    // startup. Announce whatever is already stored, or that
                    // first track would sit unadvertised until the next change.
                    announce();
                    loop {
                        std::thread::park();
                    }
                }
                Err(e) => {
                    // No session bus is normal on a headless machine.
                    crate::plog_info!("[media-keys] MPRIS unavailable: {}", e);
                }
            }
        })
        .ok();
}

/// Record what the app is playing and tell the bus about it.
///
/// Called from the event loop, not from the MPRIS thread. Storing always
/// succeeds; announcing is skipped when the connection is not up yet, and
/// `start` announces once it is - so a track published during startup is not
/// lost.
pub fn publish(info: &NowPlayingInfo) {
    {
        let mut guard = SESSION
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // A NEW TRACK NEEDS A NEW `mpris:trackid`, and a pause must not get
        // one: a desktop keys its progress bar and its track-change
        // notification on that id, so minting one per publish would reset the
        // bar and pop a notification every time the user hit pause.
        let new_track = guard
            .as_ref()
            .map_or(true, |old| old.is_different_track(info));
        if new_track {
            TRACK_SERIAL.fetch_add(1, Ordering::Relaxed);
        }
        *guard = Some(info.clone());
    }
    announce();
}

/// Emit `PropertiesChanged` for the properties that just changed.
///
/// Raw signal rather than zbus's generated `*_changed` helpers: those are async
/// methods on the interface and reaching them from a blocking context means
/// taking an `InterfaceRef` and driving a future from the event-loop thread.
/// The signal is three arguments and is what those helpers send anyway.
///
/// `Position` is NOT in the map, and that is the spec's rule rather than an
/// omission - see the module docs.
fn announce() {
    let Some(conn) = CONN.get() else {
        return;
    };
    let mut changed: std::collections::HashMap<String, zbus::zvariant::OwnedValue> =
        std::collections::HashMap::new();

    let status = match session().map(|s| s.state) {
        Some(MediaPlaybackState::Playing) => "Playing",
        Some(MediaPlaybackState::Paused) => "Paused",
        _ => "Stopped",
    };
    if let Ok(v) = zbus::zvariant::OwnedValue::try_from(zbus::zvariant::Value::from(
        status.to_string(),
    )) {
        changed.insert("PlaybackStatus".to_string(), v);
    }
    // `Metadata` is `a{sv}`, so the value in the changed map is a VARIANT
    // CONTAINING A DICT - one level deeper than the property itself. Passing
    // the dict directly produces a signature clients reject.
    if let Ok(v) = zbus::zvariant::OwnedValue::try_from(zbus::zvariant::Value::from(metadata_map()))
    {
        changed.insert("Metadata".to_string(), v);
    }

    let invalidated: Vec<String> = Vec::new();
    if let Err(e) = conn.emit_signal(
        None::<&str>,
        OBJECT_PATH,
        "org.freedesktop.DBus.Properties",
        "PropertiesChanged",
        &("org.mpris.MediaPlayer2.Player", changed, invalidated),
    ) {
        // A dead bus is not worth failing a frame over.
        crate::plog_info!("[media-session] PropertiesChanged failed: {}", e);
    }
}
