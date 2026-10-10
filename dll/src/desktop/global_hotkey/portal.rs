//! Wayland global hotkeys: the xdg-desktop-portal `GlobalShortcuts`
//! interface (`org.freedesktop.portal.GlobalShortcuts`, portal 1.17+).
//!
//! # Why not a key grab
//!
//! A Wayland client cannot see a key pressed while another client has the
//! keyboard focus - by design, and there is no protocol to ask. The portal
//! is the sanctioned route: the app asks the DESKTOP to bind shortcuts, the
//! desktop may show the user a dialog (and may let them pick a different
//! trigger), and it reports each activation as a D-Bus signal. Implemented by
//! KDE Plasma (5.27+), GNOME (48+) and Hyprland's portal; where no backend
//! implements the interface the probe says so and every grab answers
//! `Unavailable`.
//!
//! # Shape: one session per BATCH, under stable ids
//!
//! The interface is bind-once per session and may ask the user per bind, so
//! a grab is only QUEUED by `register`; `commit` - the end of the manager's
//! reconcile batch - binds everything queued in ONE new session (one dialog
//! for N hotkeys), with the canonical accelerator as each shortcut's id so
//! the desktop recognises on the next launch what it approved on this one.
//! A release tombstones its shortcut (its activations are dropped); a
//! session with nothing left is closed, a partly released one is folded into
//! the next batch's session. The planning is the pure `portal_plan`
//! module, tested on every host.
//!
//! The handshake waits on the user, so it runs on a thread of its own; the
//! grabs of the batch are `Pending` until it answers through the backend's
//! [`HotkeySink`]. One long-lived listener thread per backend receives every
//! `Activated` signal and parks a press for the live shortcut it names.
//!
//! # D-Bus
//!
//! Through `zbus` (blocking), the client the repo's other portal users
//! already share (`eyedropper/wayland.rs`, `extra/permission/linux.rs`,
//! `extra/screencap/linux.rs`): the tray's dlopen'd libdbus table has no way
//! to RECEIVE a signal (no match rules, no filter - its own docs say so), and
//! both the `Response` of every portal request and `Activated` are signals.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock, PoisonError},
};

use azul_core::global_hotkey::{GlobalHotkey, GlobalHotkeyError, GlobalHotkeyId};
use azul_layout::managers::global_hotkey::{
    BackendEvent, BackendGrant, GlobalHotkeyBackend, HotkeySink,
};

use super::portal_plan::{plan_commit, shortcut_id, PlannedShortcut, SessionView};

const DESKTOP_NAME: &str = "org.freedesktop.portal.Desktop";
const DESKTOP_PATH: &str = "/org/freedesktop/portal/desktop";
const SHORTCUTS_IFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
const REQUEST_IFACE: &str = "org.freedesktop.portal.Request";
const SESSION_IFACE: &str = "org.freedesktop.portal.Session";

/// What the capability probe and the backend call themselves.
pub(super) const NAME: &str = "xdg-desktop-portal GlobalShortcuts";

/// The session bus, shared by the handshakes and the listener.
fn connection() -> Option<&'static zbus::blocking::Connection> {
    static CONNECTION: OnceLock<Option<zbus::blocking::Connection>> = OnceLock::new();
    CONNECTION
        .get_or_init(|| zbus::blocking::Connection::session().ok())
        .as_ref()
}

fn platform(what: &str, e: impl core::fmt::Display) -> GlobalHotkeyError {
    GlobalHotkeyError::Platform(format!("{what}: {e}").into())
}

/// Is the `GlobalShortcuts` interface there? Asked once (a D-Bus round
/// trip), then cached for the process.
pub(super) fn probe() -> Result<(), String> {
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

/// `CreateSession` + `BindShortcuts` for one batch. Blocks for the user's
/// answer; runs on its own thread. Returns the session path.
fn handshake(session_key: u64, shortcuts: &[PlannedShortcut]) -> Result<String, GlobalHotkeyError> {
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
    let session_token = format!("azul_hotkeys_{pid}_{session_key}");
    let session_path = format!("{DESKTOP_PATH}/session/{sender}/{session_token}");
    let create_token = format!("azul_hotkeys_create_{pid}_{session_key}");
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

    // 2. BindShortcuts, the whole batch at once. This is where a desktop may
    //    ask the user, and where the answer can take a while.
    let bind_token = format!("azul_hotkeys_bind_{pid}_{session_key}");
    let bind_path = format!("{DESKTOP_PATH}/request/{sender}/{bind_token}");
    let bind_request =
        zbus::blocking::Proxy::new(conn, DESKTOP_NAME, bind_path.as_str(), REQUEST_IFACE)
            .map_err(|e| platform("BindShortcuts request", e))?;
    let mut bind_responses = bind_request
        .receive_signal("Response")
        .map_err(|e| platform("BindShortcuts request", e))?;
    let mut bound_shortcuts: Vec<(&str, HashMap<&str, Value<'_>>)> = Vec::new();
    for planned in shortcuts {
        let mut shortcut: HashMap<&str, Value<'_>> = HashMap::new();
        shortcut.insert("description", Value::from(planned.description.as_str()));
        shortcut.insert("preferred_trigger", Value::from(planned.trigger.as_str()));
        bound_shortcuts.push((planned.shortcut_id.as_str(), shortcut));
    }
    let mut bind_options: HashMap<&str, Value<'_>> = HashMap::new();
    bind_options.insert("handle_token", Value::from(bind_token.as_str()));
    let session = ObjectPath::try_from(session_path.as_str())
        .map_err(|e| platform("the session path", e))?;
    let bound: Result<OwnedObjectPath, zbus::Error> =
        proxy.call("BindShortcuts", &(session, bound_shortcuts, "", bind_options));
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
                format!("the portal did not bind the shortcuts (response {other})").into(),
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

/// Close sessions off the event-loop thread (each is a D-Bus round trip).
fn close_sessions_later(paths: Vec<String>) {
    if paths.is_empty() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name(String::from("azul-hotkey-close"))
        .spawn(move || {
            for path in paths {
                close_session(&path);
            }
        });
}

/// One shortcut of a session.
struct SessionShortcut {
    planned: PlannedShortcut,
    /// Still wanted; a released one is a tombstone whose activations are
    /// dropped (a shortcut cannot be unbound from a live session).
    live: bool,
}

/// One portal session this backend opened.
struct Session {
    key: u64,
    /// Known once the handshake bound it; `None` while it runs.
    path: Option<String>,
    shortcuts: Vec<SessionShortcut>,
}

/// Shared with the handshake threads and the listener.
#[derive(Default)]
struct PortalState {
    sessions: Vec<Session>,
    next_key: u64,
}

type SharedState = Arc<Mutex<PortalState>>;

fn lock(state: &SharedState) -> std::sync::MutexGuard<'_, PortalState> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Start this backend's `Activated` listener. It must be subscribed BEFORE
/// the first bind, so no activation is missed.
fn start_listener(state: SharedState, sink: HotkeySink) {
    let spawned = std::thread::Builder::new()
        .name(String::from("azul-hotkey-portal"))
        .spawn(move || {
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
                let Ok((session, shortcut, timestamp, _options)) =
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
                let os_id = {
                    let state = lock(&state);
                    state
                        .sessions
                        .iter()
                        .filter(|s| s.path.as_deref() == Some(session.as_str()))
                        .flat_map(|s| s.shortcuts.iter())
                        .find(|s| s.live && s.planned.shortcut_id == shortcut)
                        .map(|s| s.planned.os_id)
                };
                if let Some(id) = os_id {
                    sink.push(BackendEvent::Fired {
                        os_id: GlobalHotkeyId { id },
                        state: azul_core::global_hotkey::GlobalHotkeyState::Pressed,
                        timestamp_ms: timestamp,
                    });
                }
            }
        });
    if let Err(e) = spawned {
        crate::plog_warn!("[global-hotkey] could not start the portal listener: {e}");
    }
}

/// One app's portal shortcuts.
pub(super) struct PortalBackend {
    sink: HotkeySink,
    state: SharedState,
    /// Registered since the last `commit`: bound together there.
    queued: Vec<PlannedShortcut>,
    listener_started: bool,
}

impl PortalBackend {
    pub(super) fn new(sink: HotkeySink) -> Self {
        Self {
            sink,
            state: Arc::new(Mutex::new(PortalState::default())),
            queued: Vec::new(),
            listener_started: false,
        }
    }
}

/// Bind `shortcuts` in session `key` on a thread of its own, then report:
/// every shortcut of the batch that is still wanted settles `Active` (or the
/// refusal, for the ones the batch was FOR - survivors folded in from an
/// older session keep working there), and the older sessions the batch
/// replaces are closed once it bound.
fn spawn_bind(
    state: SharedState,
    sink: HotkeySink,
    key: u64,
    shortcuts: Vec<PlannedShortcut>,
    fresh: Vec<u32>,
    replaces: Vec<u64>,
) -> Result<(), GlobalHotkeyError> {
    std::thread::Builder::new()
        .name(format!("azul-hotkey-bind-{key}"))
        .spawn(move || match handshake(key, &shortcuts) {
            Ok(path) => {
                let (live, stale_paths, orphaned) = {
                    let mut portal = lock(&state);
                    let mut live: Vec<u32> = Vec::new();
                    let mut orphaned = true;
                    if let Some(session) = portal.sessions.iter_mut().find(|s| s.key == key) {
                        orphaned = false;
                        session.path = Some(path.clone());
                        live = session
                            .shortcuts
                            .iter()
                            .filter(|s| s.live)
                            .map(|s| s.planned.os_id)
                            .collect();
                    }
                    let mut stale_paths: Vec<String> = Vec::new();
                    if !orphaned {
                        portal.sessions.retain(|s| {
                            if replaces.contains(&s.key) {
                                if let Some(p) = s.path.clone() {
                                    stale_paths.push(p);
                                }
                                false
                            } else {
                                true
                            }
                        });
                    }
                    (live, stale_paths, orphaned)
                };
                if orphaned {
                    // Every shortcut was released while the desktop asked.
                    close_session(&path);
                    return;
                }
                for id in live {
                    sink.push(BackendEvent::Settled {
                        os_id: GlobalHotkeyId { id },
                        result: Ok(azul_css::AzString::from_const_str("")),
                    });
                }
                for stale in stale_paths {
                    close_session(&stale);
                }
            }
            Err(e) => {
                crate::plog_warn!("[global-hotkey] the portal did not bind the batch: {e}");
                lock(&state).sessions.retain(|s| s.key != key);
                for id in fresh {
                    sink.push(BackendEvent::Settled {
                        os_id: GlobalHotkeyId { id },
                        result: Err(e.clone()),
                    });
                }
            }
        })
        .map(|_| ())
        .map_err(|e| platform("could not start the portal handshake", e))
}

impl GlobalHotkeyBackend for PortalBackend {
    fn name(&self) -> &'static str {
        NAME
    }

    fn probe(&self) -> Result<(), String> {
        probe()
    }

    fn register(
        &mut self,
        os_id: GlobalHotkeyId,
        hotkey: &GlobalHotkey,
        description: &str,
    ) -> Result<BackendGrant, GlobalHotkeyError> {
        if let Err(why) = probe() {
            return Err(GlobalHotkeyError::Unavailable(why.into()));
        }
        let Some(id) = shortcut_id(hotkey) else {
            return Err(GlobalHotkeyError::KeyNotMappable);
        };
        if !self.listener_started {
            self.listener_started = true;
            start_listener(self.state.clone(), self.sink.clone());
        }
        // Queued: `commit` binds the whole batch in one session.
        self.queued.push(PlannedShortcut {
            os_id: os_id.id,
            trigger: id.clone(),
            shortcut_id: id,
            description: description.to_string(),
        });
        Ok(BackendGrant::Pending)
    }

    fn unregister(&mut self, os_id: GlobalHotkeyId) {
        // Never bound yet: just forget it.
        let before = self.queued.len();
        self.queued.retain(|q| q.os_id != os_id.id);
        if self.queued.len() != before {
            return;
        }
        // Bound (or binding): tombstone it; `commit` closes or compacts.
        for session in &mut lock(&self.state).sessions {
            for shortcut in &mut session.shortcuts {
                if shortcut.planned.os_id == os_id.id {
                    shortcut.live = false;
                }
            }
        }
    }

    fn commit(&mut self) {
        let queued = core::mem::take(&mut self.queued);
        let (key, plan, close_now) = {
            let mut portal = lock(&self.state);
            let views: Vec<SessionView> = portal
                .sessions
                .iter()
                .map(|s| SessionView {
                    key: s.key,
                    live: s
                        .shortcuts
                        .iter()
                        .filter(|sc| sc.live)
                        .map(|sc| sc.planned.clone())
                        .collect(),
                    released: s.shortcuts.iter().filter(|sc| !sc.live).count(),
                })
                .collect();
            let plan = plan_commit(&views, &queued);
            // Sessions with nothing left: out of the table now (a handshake
            // still running for one finds it gone and closes what it bound).
            let mut close_now: Vec<String> = Vec::new();
            portal.sessions.retain(|s| {
                if plan.close_now.contains(&s.key) {
                    if let Some(p) = s.path.clone() {
                        close_now.push(p);
                    }
                    false
                } else {
                    true
                }
            });
            let key = if plan.bind.is_empty() {
                None
            } else {
                portal.next_key += 1;
                let key = portal.next_key;
                portal.sessions.push(Session {
                    key,
                    path: None,
                    shortcuts: plan
                        .bind
                        .iter()
                        .map(|planned| SessionShortcut {
                            planned: planned.clone(),
                            live: true,
                        })
                        .collect(),
                });
                Some(key)
            };
            (key, plan, close_now)
        };
        close_sessions_later(close_now);
        let Some(key) = key else {
            return;
        };
        let fresh: Vec<u32> = queued.iter().map(|q| q.os_id).collect();
        if let Err(e) = spawn_bind(
            self.state.clone(),
            self.sink.clone(),
            key,
            plan.bind,
            fresh.clone(),
            plan.close_after_bind,
        ) {
            lock(&self.state).sessions.retain(|s| s.key != key);
            for id in fresh {
                self.sink.push(BackendEvent::Settled {
                    os_id: GlobalHotkeyId { id },
                    result: Err(e.clone()),
                });
            }
        }
    }

    fn needs_loop_polling(&self) -> bool {
        // Presses arrive on the listener thread: the loop needs a waker on
        // the sink, or it polls.
        true
    }
}

impl Drop for PortalBackend {
    /// Dropping the App (or replacing the backend) closes every session.
    fn drop(&mut self) {
        let paths: Vec<String> = core::mem::take(&mut lock(&self.state).sessions)
            .into_iter()
            .filter_map(|s| s.path)
            .collect();
        close_sessions_later(paths);
    }
}
