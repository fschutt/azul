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
//! # The properties are stubbed, deliberately
//!
//! MPRIS requires `PlaybackStatus`, `Metadata` and the `Can*` flags. Azul has
//! no playback state machine (11c is blocked on exactly that), so they report a
//! stopped player with no track. That is honest rather than convenient: the
//! desktop shows transport buttons and no title, which is what an app with no
//! metadata to give SHOULD look like. Filling them in is 9h-i-a-i.

use azul_core::window::VirtualKeyCode;
use azul_layout::managers::media_keys::push_media_key;

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

    /// `Stopped`, because azul has no playback state to report.
    ///
    /// This is REQUIRED by the spec and read by every desktop; omitting it
    /// makes some of them treat the player as broken and hide it, taking the
    /// transport buttons with it - so a stub is needed for the keys to work
    /// at all, not merely for tidiness.
    #[zbus(property)]
    fn playback_status(&self) -> String {
        "Stopped".to_string()
    }

    /// Empty metadata: no track, no title, no art. An app that has something
    /// to say here needs the state machine 11c is blocked on.
    #[zbus(property)]
    fn metadata(&self) -> std::collections::HashMap<String, zbus::zvariant::OwnedValue> {
        std::collections::HashMap::new()
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
    /// `Raise` is the desktop asking to focus the app from its media widget.
    /// Not wired to a window: raising is a window-manager action the shell
    /// owns, and inventing one from here would fight it. Logged as 9h-i-a-ii.
    fn raise(&self) {}

    /// NOT wired to a quit. A media widget's close button terminating the app
    /// would be a surprise; `CanQuit` reports false so no desktop offers it.
    fn quit(&self) {}

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
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
pub fn start() {
    if !azul_layout::window::expose_system_media_controls() {
        return;
    }
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
                    // released the moment it drops and the desktop forgets
                    // the player. Parking the thread is what keeps it alive;
                    // the interfaces are served from zbus's own executor.
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
