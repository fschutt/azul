//! Wayland global hotkeys: the xdg-desktop-portal `GlobalShortcuts`
//! interface (`org.freedesktop.portal.GlobalShortcuts`, portal 1.17+).
//!
//! # Why not a key grab
//!
//! A Wayland client cannot see a key pressed while another client has the
//! keyboard focus - by design, and there is no protocol to ask. The portal
//! is the sanctioned route: the app asks the DESKTOP to bind a shortcut, the
//! desktop may show the user a dialog (and may let them pick a different
//! trigger), and it reports each activation as a D-Bus signal. Implemented by
//! KDE Plasma (5.27+), GNOME (48+) and Hyprland's portal; where no backend
//! implements the interface the probe says so and registration answers
//! `Unavailable`.
//!
//! # Shape
//!
//! - One portal SESSION per registration (`CreateSession` then `BindShortcuts` with one
//!   shortcut), so unregistering is `Session.Close` and nothing else - no re-binding of a shared
//!   set, which some implementations refuse after the first bind.
//! - The handshake may wait on the user's answer to a dialog, so it runs on a thread of its own
//!   and the registration is `Pending` until it answers; `report` then settles it `Active`, or
//!   `Failed(Denied)` / `Failed(Platform)`.
//! - One long-lived listener thread receives every `Activated` signal and parks the id of the
//!   session it names.
//!
//! # D-Bus
//!
//! Through `zbus` (blocking), the client the repo's other portal users
//! already share (`eyedropper/wayland.rs`, `extra/permission/linux.rs`,
//! `extra/screencap/linux.rs`): the tray's dlopen'd libdbus table has no way
//! to RECEIVE a signal (no match rules, no filter - its own docs say so), and
//! both the `Response` of every portal request and `Activated` are signals.

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Mutex, OnceLock, PoisonError},
};

use azul_core::global_hotkey::{portal_trigger, GlobalHotkey, GlobalHotkeyError, GlobalHotkeyId};
use azul_layout::managers::global_hotkey::{
    is_registered, push_fired, report, BackendGrant, GlobalHotkeyBackend,
};

const DESKTOP_NAME: &str = "org.freedesktop.portal.Desktop";
const DESKTOP_PATH: &str = "/org/freedesktop/portal/desktop";
const SHORTCUTS_IFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
const REQUEST_IFACE: &str = "org.freedesktop.portal.Request";
const SESSION_IFACE: &str = "org.freedesktop.portal.Session";

/// The session bus, shared by the handshakes and the listener.
fn connection() -> Option<&'static zbus::blocking::Connection> {
    static CONNECTION: OnceLock<Option<zbus::blocking::Connection>> = OnceLock::new();
    CONNECTION
        .get_or_init(|| zbus::blocking::Connection::session().ok())
        .as_ref()
}

/// Live sessions, by registry id: the session object path.
static SESSIONS: Mutex<BTreeMap<u32, String>> = Mutex::new(BTreeMap::new());

fn sessions() -> std::sync::MutexGuard<'static, BTreeMap<u32, String>> {
    SESSIONS.lock().unwrap_or_else(PoisonError::into_inner)
}

fn platform(what: &str, e: impl core::fmt::Display) -> GlobalHotkeyError {
    GlobalHotkeyError::Platform(format!("{what}: {e}").into())
}

/// Is the `GlobalShortcuts` interface there? Asked once (a D-Bus round
/// trip), then cached for the process.
fn probe() -> Result<(), String> {
    static PROBED: OnceLock<Result<(), String>> = OnceLock::new();
    PROBED
        .get_or_init(|| {
            let conn = connection()
                .ok_or_else(|| String::from("no D-Bus session bus, so no desktop portal"))?;
            let proxy =
                zbus::blocking::Proxy::new(conn, DESKTOP_NAME, DESKTOP_PATH, SHORTCUTS_IFACE)
                    .map_err(|e| format!("the desktop portal is not reachable ({e})"))?;
            let version: u32 = proxy.get_property("version").map_err(|e| {
                format!(
                    "xdg-desktop-portal has no GlobalShortcuts interface ({e}): it needs \
                     xdg-desktop-portal 1.17+ and a desktop backend that implements it (KDE \
                     Plasma 5.27+, GNOME 48+, Hyprland)"
                )
            })?;
            if version == 0 {
                return Err(String::from(
                    "the GlobalShortcuts portal reports interface version 0",
                ));
            }
            Ok(())
        })
        .clone()
}

/// The portal predicts request and session object paths from the caller's
/// unique name: `:1.42` becomes `1_42`.
fn sender_token(conn: &zbus::blocking::Connection) -> Option<String> {
    conn.unique_name()
        .map(|name| name.to_string().trim_start_matches(':').replace('.', "_"))
}

/// Wait for the `Response` of one portal request. `subscribe` is called
/// BEFORE `call`, because the answer can come back immediately.
fn await_response(
    responses: &mut zbus::blocking::proxy::SignalIterator<'_>,
    what: &str,
) -> Result<u32, GlobalHotkeyError> {
    let Some(message) = responses.next() else {
        return Err(GlobalHotkeyError::Platform(
            format!("the portal closed the {what} request without answering").into(),
        ));
    };
    let (code, _results) = message
        .body()
        .deserialize::<(u32, HashMap<String, zbus::zvariant::OwnedValue>)>()
        .map_err(|e| platform(what, e))?;
    Ok(code)
}

/// `CreateSession` + `BindShortcuts` for one registration. Blocks for the
/// user's answer; runs on its own thread. Returns the session path.
fn handshake(
    id: GlobalHotkeyId,
    trigger: &str,
    description: &str,
) -> Result<String, GlobalHotkeyError> {
    use zbus::zvariant::{ObjectPath, OwnedObjectPath, Value};

    let conn = connection().ok_or_else(|| {
        GlobalHotkeyError::Unavailable("no D-Bus session bus".into())
    })?;
    let sender = sender_token(conn).ok_or_else(|| {
        GlobalHotkeyError::Platform("the session bus gave this app no unique name".into())
    })?;
    let proxy = zbus::blocking::Proxy::new(conn, DESKTOP_NAME, DESKTOP_PATH, SHORTCUTS_IFACE)
        .map_err(|e| platform("GlobalShortcuts", e))?;
    let pid = std::process::id();

    // 1. CreateSession. The session path is predicted from the token, like
    //    the request path; the `session_handle` in the answer names it too.
    let session_token = format!("azul_hotkey_{pid}_{}", id.id);
    let session_path = format!("{DESKTOP_PATH}/session/{sender}/{session_token}");
    let create_token = format!("azul_hotkey_create_{pid}_{}", id.id);
    let create_path = format!("{DESKTOP_PATH}/request/{sender}/{create_token}");
    let create_request =
        zbus::blocking::Proxy::new(conn, DESKTOP_NAME, create_path.as_str(), REQUEST_IFACE)
            .map_err(|e| platform("CreateSession request", e))?;
    let mut create_responses = create_request
        .receive_signal("Response")
        .map_err(|e| platform("CreateSession request", e))?;
    let mut create_options: HashMap<&str, Value<'_>> = HashMap::new();
    create_options.insert("handle_token", Value::from(create_token.as_str()));
    create_options.insert("session_handle_token", Value::from(session_token.as_str()));
    let _: OwnedObjectPath = proxy
        .call("CreateSession", &create_options)
        .map_err(|e| platform("CreateSession", e))?;
    let code = await_response(&mut create_responses, "CreateSession")?;
    if code != 0 {
        return Err(GlobalHotkeyError::Platform(
            format!("the portal refused to create a session (response {code})").into(),
        ));
    }

    // 2. BindShortcuts, one shortcut. This is where a desktop may ask the
    //    user, and where the answer can take a while.
    let bind_token = format!("azul_hotkey_bind_{pid}_{}", id.id);
    let bind_path = format!("{DESKTOP_PATH}/request/{sender}/{bind_token}");
    let bind_request =
        zbus::blocking::Proxy::new(conn, DESKTOP_NAME, bind_path.as_str(), REQUEST_IFACE)
            .map_err(|e| platform("BindShortcuts request", e))?;
    let mut bind_responses = bind_request
        .receive_signal("Response")
        .map_err(|e| platform("BindShortcuts request", e))?;
    let shortcut_id = format!("azul-hotkey-{}", id.id);
    let mut shortcut: HashMap<&str, Value<'_>> = HashMap::new();
    shortcut.insert("description", Value::from(description));
    shortcut.insert("preferred_trigger", Value::from(trigger));
    let shortcuts = vec![(shortcut_id.as_str(), shortcut)];
    let mut bind_options: HashMap<&str, Value<'_>> = HashMap::new();
    bind_options.insert("handle_token", Value::from(bind_token.as_str()));
    let session = ObjectPath::try_from(session_path.as_str())
        .map_err(|e| platform("the session path", e))?;
    let bound: Result<OwnedObjectPath, zbus::Error> =
        proxy.call("BindShortcuts", &(session, shortcuts, "", bind_options));
    if let Err(e) = bound {
        close_session(&session_path);
        return Err(platform("BindShortcuts", e));
    }
    match await_response(&mut bind_responses, "BindShortcuts") {
        Ok(0) => Ok(session_path),
        Ok(1) => {
            close_session(&session_path);
            Err(GlobalHotkeyError::Denied)
        }
        Ok(other) => {
            close_session(&session_path);
            Err(GlobalHotkeyError::Platform(
                format!("the portal did not bind the shortcut (response {other})").into(),
            ))
        }
        Err(e) => {
            close_session(&session_path);
            Err(e)
        }
    }
}

/// `org.freedesktop.portal.Session.Close`. Best effort.
fn close_session(session_path: &str) {
    let Some(conn) = connection() else {
        return;
    };
    if let Ok(session) =
        zbus::blocking::Proxy::new(conn, DESKTOP_NAME, session_path, SESSION_IFACE)
    {
        let _: Result<(), zbus::Error> = session.call("Close", &());
    }
}

/// Start the `Activated` listener, once. It must be subscribed BEFORE the
/// first bind, so no activation is missed.
fn ensure_listener() {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name(String::from("azul-hotkey-portal"))
        .spawn(|| {
            let Some(conn) = connection() else {
                return;
            };
            let Ok(proxy) =
                zbus::blocking::Proxy::new(conn, DESKTOP_NAME, DESKTOP_PATH, SHORTCUTS_IFACE)
            else {
                return;
            };
            let Ok(activations) = proxy.receive_signal("Activated") else {
                crate::plog_warn!(
                    "[global-hotkey] could not subscribe to the portal's Activated signal"
                );
                return;
            };
            for message in activations {
                // (o session_handle, s shortcut_id, t timestamp, a{sv} options)
                let Ok((session, _shortcut_id, _timestamp, _options)) =
                    message.body().deserialize::<(
                        zbus::zvariant::OwnedObjectPath,
                        String,
                        u64,
                        HashMap<String, zbus::zvariant::OwnedValue>,
                    )>()
                else {
                    continue;
                };
                let session = session.to_string();
                let id = sessions()
                    .iter()
                    .find(|(_, path)| **path == session)
                    .map(|(id, _)| *id);
                if let Some(id) = id {
                    push_fired(GlobalHotkeyId { id });
                    // This thread cannot run the callback, and the loop is
                    // parked in poll(2): the waker's fd is in its set.
                    crate::desktop::loop_waker::wake();
                }
            }
        });
    if let Err(e) = spawned {
        crate::plog_warn!("[global-hotkey] could not start the portal listener: {e}");
    }
}

fn register(id: GlobalHotkeyId, hotkey: &GlobalHotkey) -> Result<BackendGrant, GlobalHotkeyError> {
    if let Err(why) = probe() {
        return Err(GlobalHotkeyError::Unavailable(why.into()));
    }
    let Some(trigger) = portal_trigger(hotkey) else {
        return Err(GlobalHotkeyError::KeyNotMappable);
    };
    ensure_listener();
    let description = format!(
        "Global hotkey {}",
        hotkey.to_display_string().as_str()
    );
    std::thread::Builder::new()
        .name(format!("azul-hotkey-bind-{}", id.id))
        .spawn(move || match handshake(id, &trigger, &description) {
            Ok(session_path) => {
                if is_registered(id) {
                    sessions().insert(id.id, session_path);
                    report(id, Ok(()));
                    crate::desktop::loop_waker::wake();
                } else {
                    // Unregistered while the desktop was asking the user.
                    close_session(&session_path);
                }
            }
            Err(e) => {
                crate::plog_warn!("[global-hotkey] the portal did not bind {trigger}: {e}");
                report(id, Err(e));
                crate::desktop::loop_waker::wake();
            }
        })
        .map_err(|e| platform("could not start the portal handshake", e))?;
    Ok(BackendGrant::Pending)
}

fn unregister(id: GlobalHotkeyId) {
    let Some(session_path) = sessions().remove(&id.id) else {
        return;
    };
    // A D-Bus round trip: off the event-loop thread.
    let _ = std::thread::Builder::new()
        .name(String::from("azul-hotkey-close"))
        .spawn(move || close_session(&session_path));
}

/// Nothing to poll: the listener thread parks the fires.
fn poll() {}

pub(super) fn backend() -> GlobalHotkeyBackend {
    GlobalHotkeyBackend {
        name: "xdg-desktop-portal GlobalShortcuts",
        probe,
        register,
        unregister,
        poll,
        // Fires arrive on the listener thread, which wakes the loop through
        // `desktop::loop_waker::wake` right after parking them.
        needs_loop_polling: false,
    }
}
