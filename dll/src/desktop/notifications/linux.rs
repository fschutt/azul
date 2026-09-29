//! Linux native notifications  -  `org.freedesktop.Notifications` over D-Bus.
//!
//! The tray's transport, reused as-is: the dlopen'd libdbus the whole Linux
//! shell shares (`get_shared_dbus_lib`) and the process's shared session
//! connection (`dbus_bus_get`), which the tray's pump dispatches too. Works the
//! same under X11 and Wayland - the notification server is a D-Bus service,
//! not a display-server protocol.
//!
//! # The protocol (Desktop Notifications spec 1.2)
//!
//! * `Notify(app_name s, replaces_id u, app_icon s, summary s, body s, actions as, hints a{sv},
//!   expire_timeout i) -> u`: the SERVER picks the id. [`SERVER_IDS`] maps it back to the app's
//!   string id, and `replaces_id` re-uses it when the app posts the same id again.
//! * `CloseNotification(u)`: withdraw.
//! * `ActionInvoked(u, s)`: a button - or, for the action key `default`, a click on the body. That
//!   key is only reported if it is in `actions`, so `wire::freedesktop_actions` always puts it
//!   first.
//! * `NotificationClosed(u, u)`: expired / dismissed / closed. A server sends it AFTER
//!   `ActionInvoked` for the same click; the registry delivers only the first (every event ends
//!   its notification).
//! * `ActivationToken(u, s)` (spec 1.2, GNOME and KDE send it) arrives just BEFORE the
//!   `ActionInvoked` of a click. On Wayland it is the only thing that lets the app raise its
//!   window - a click on a notification carries no input serial - so it is kept for the run loop
//!   ([`take_activation_token`]), which hands it to `xdg_activation_v1.activate`.
//!
//! # Identity
//!
//! Every `Notify` carries the `desktop-entry` hint: the executable's name, the same default the
//! Wayland `app_id` and the X11 `WM_CLASS` use (`wire::desktop_entry`). GNOME matches a sender by
//! its window's PID first; a windowless or tray-only process is matched by this hint, and without
//! a match its notifications get a generic source with no per-app settings.
//!
//! # Receiving the signals
//!
//! GNOME Shell BROADCASTS the two signals, dunst sends them to the caller
//! only; a match rule (`dbus_bus_add_match`) makes the bus route both kinds to
//! this connection, and a filter (`dbus_connection_add_filter`) sees them as
//! the connection is dispatched - by [`PlatformNotifier::pump`], and by the
//! tray's pump. Signals for other applications' notifications carry ids that
//! are not in [`SERVER_IDS`] and are ignored. The filter never claims a
//! message, so the tray and the GNOME menu handlers on the same connection
//! see everything they saw before.
//!
//! # No server
//!
//! A session without a notification daemon (a bare window manager with no
//! dunst/mako) has no `org.freedesktop.Notifications`, and D-Bus activation
//! cannot start one. `GetServerInformation` fails, the backend is unavailable
//! with that reason, and posts become `Failed` events - never a silent no-op.

use std::{
    collections::BTreeMap,
    ffi::{CStr, CString},
    os::raw::{c_char, c_int, c_uint, c_void},
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use azul_core::notification::{Notification, NotificationSound};
use azul_layout::managers::notification::{queue_notification_event, wire};

use crate::desktop::shell2::linux::{
    dbus::{
        DBusConnection, DBusError, DBusLib, DBusMessage, DBusMessageIter, DBUS_BUS_SESSION,
        DBUS_HANDLER_RESULT_NOT_YET_HANDLED, DBUS_TYPE_ARRAY, DBUS_TYPE_BOOLEAN,
        DBUS_TYPE_DICT_ENTRY, DBUS_TYPE_INT32, DBUS_TYPE_STRING, DBUS_TYPE_UINT32,
        DBUS_TYPE_VARIANT,
    },
    gnome_menu::get_shared_dbus_lib,
};

const DEST: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const IFACE: &str = "org.freedesktop.Notifications";
/// `Notify` blocks for the reply (the server's id). D-Bus activation of a
/// daemon that is installed but not running can take a moment.
const NOTIFY_TIMEOUT_MS: c_int = 3000;
/// The capability probe must not stall a layout callback that asks.
const PROBE_TIMEOUT_MS: c_int = 1000;

/// Server id -> the app's id. Written by `post`, read by the filter (which
/// runs inside a dispatch and has no other state), cleared on close.
static SERVER_IDS: Mutex<BTreeMap<u32, String>> = Mutex::new(BTreeMap::new());

/// The `ActivationToken` of the last click on one of this app's
/// notifications, until the run loop takes it.
static ACTIVATION_TOKEN: Mutex<Option<String>> = Mutex::new(None);

/// Take the activation token a click brought (see the module docs).
pub(super) fn take_activation_token() -> Option<String> {
    ACTIVATION_TOKEN
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
}

fn fresh_error() -> DBusError {
    DBusError {
        name: std::ptr::null(),
        message: std::ptr::null(),
        dummy1: 0,
        dummy2: 0,
        dummy3: 0,
        dummy4: 0,
        dummy5: 0,
        padding1: std::ptr::null_mut(),
    }
}

/// The message of a set error (and frees it), or `fallback`.
unsafe fn take_error(dbus: &DBusLib, err: &mut DBusError, fallback: &str) -> String {
    let message = if unsafe { (dbus.dbus_error_is_set)(&*err) } != 0 && !err.message.is_null() {
        unsafe { CStr::from_ptr(err.message) }
            .to_string_lossy()
            .into_owned()
    } else {
        fallback.to_string()
    };
    unsafe { (dbus.dbus_error_free)(&mut *err) };
    message
}

/// The shared session connection (`dbus_bus_get` hands every caller the same
/// one, reference-counted - the tray uses it too).
unsafe fn session(dbus: &DBusLib) -> Result<*mut DBusConnection, String> {
    let mut err = fresh_error();
    unsafe { (dbus.dbus_error_init)(&mut err) };
    let conn = unsafe { (dbus.dbus_bus_get)(DBUS_BUS_SESSION, &mut err) };
    if conn.is_null() {
        return Err(unsafe { take_error(dbus, &mut err, "no D-Bus session bus") });
    }
    Ok(conn)
}

// ---- marshalling -------------------------------------------------------------

unsafe fn append_str(dbus: &DBusLib, it: *mut DBusMessageIter, s: &str) {
    let c = CString::new(s.replace('\0', "")).unwrap_or_default();
    let p = c.as_ptr();
    unsafe {
        (dbus.dbus_message_iter_append_basic)(
            it,
            DBUS_TYPE_STRING,
            &p as *const *const c_char as *const c_void,
        );
    }
}

unsafe fn append_u32(dbus: &DBusLib, it: *mut DBusMessageIter, v: u32) {
    unsafe {
        (dbus.dbus_message_iter_append_basic)(
            it,
            DBUS_TYPE_UINT32,
            &v as *const u32 as *const c_void,
        );
    }
}

unsafe fn append_i32(dbus: &DBusLib, it: *mut DBusMessageIter, v: i32) {
    unsafe {
        (dbus.dbus_message_iter_append_basic)(
            it,
            DBUS_TYPE_INT32,
            &v as *const i32 as *const c_void,
        );
    }
}

/// One `{sv}` entry of the hints dictionary, the value a STRING or BOOLEAN.
enum Hint<'a> {
    Str(&'a str),
    Bool(bool),
}

unsafe fn append_hint(dbus: &DBusLib, dict: *mut DBusMessageIter, key: &str, value: Hint<'_>) {
    unsafe {
        let mut entry: DBusMessageIter = std::mem::zeroed();
        (dbus.dbus_message_iter_open_container)(
            dict,
            DBUS_TYPE_DICT_ENTRY,
            std::ptr::null(),
            &mut entry,
        );
        append_str(dbus, &mut entry, key);
        let sig = CString::new(match value {
            Hint::Str(_) => "s",
            Hint::Bool(_) => "b",
        })
        .unwrap_or_default();
        let mut variant: DBusMessageIter = std::mem::zeroed();
        (dbus.dbus_message_iter_open_container)(
            &mut entry,
            DBUS_TYPE_VARIANT,
            sig.as_ptr(),
            &mut variant,
        );
        match value {
            Hint::Str(s) => append_str(dbus, &mut variant, s),
            Hint::Bool(b) => {
                let v: c_uint = if b { 1 } else { 0 };
                (dbus.dbus_message_iter_append_basic)(
                    &mut variant,
                    DBUS_TYPE_BOOLEAN,
                    &v as *const c_uint as *const c_void,
                );
            }
        }
        (dbus.dbus_message_iter_close_container)(&mut entry, &mut variant);
        (dbus.dbus_message_iter_close_container)(dict, &mut entry);
    }
}

/// A method call on the notification server.
unsafe fn method_call(dbus: &DBusLib, member: &str) -> Result<*mut DBusMessage, String> {
    let dest = CString::new(DEST).unwrap_or_default();
    let path = CString::new(PATH).unwrap_or_default();
    let iface = CString::new(IFACE).unwrap_or_default();
    let member_c = CString::new(member).unwrap_or_default();
    let msg = unsafe {
        (dbus.dbus_message_new_method_call)(
            dest.as_ptr(),
            path.as_ptr(),
            iface.as_ptr(),
            member_c.as_ptr(),
        )
    };
    if msg.is_null() {
        return Err(format!("could not allocate the {member} call"));
    }
    Ok(msg)
}

/// Send `msg` (consumed) and wait for the reply, which the caller unrefs.
unsafe fn call(
    dbus: &DBusLib,
    conn: *mut DBusConnection,
    msg: *mut DBusMessage,
    timeout_ms: c_int,
    what: &str,
) -> Result<*mut DBusMessage, String> {
    let mut err = fresh_error();
    unsafe { (dbus.dbus_error_init)(&mut err) };
    let reply =
        unsafe { (dbus.dbus_connection_send_with_reply_and_block)(conn, msg, timeout_ms, &mut err) };
    unsafe { (dbus.dbus_message_unref)(msg) };
    if reply.is_null() {
        return Err(unsafe { take_error(dbus, &mut err, &format!("{what} got no reply")) });
    }
    unsafe { (dbus.dbus_error_free)(&mut err) };
    Ok(reply)
}

/// Every STRING argument at the top level of `msg`, in order.
unsafe fn read_strings(dbus: &DBusLib, msg: *mut DBusMessage) -> Vec<String> {
    let mut out = Vec::new();
    unsafe {
        let mut it: DBusMessageIter = std::mem::zeroed();
        if (dbus.dbus_message_iter_init)(msg, &mut it) == 0 {
            return out;
        }
        loop {
            if (dbus.dbus_message_iter_get_arg_type)(&mut it) == DBUS_TYPE_STRING {
                let mut p: *const c_char = std::ptr::null();
                (dbus.dbus_message_iter_get_basic)(&mut it, &mut p as *mut *const c_char as *mut c_void);
                if !p.is_null() {
                    out.push(CStr::from_ptr(p).to_string_lossy().into_owned());
                }
            }
            if (dbus.dbus_message_iter_next)(&mut it) == 0 {
                break;
            }
        }
    }
    out
}

/// The strings of an `as` first argument (`GetCapabilities`).
unsafe fn read_string_array(dbus: &DBusLib, msg: *mut DBusMessage) -> Vec<String> {
    let mut out = Vec::new();
    unsafe {
        let mut it: DBusMessageIter = std::mem::zeroed();
        if (dbus.dbus_message_iter_init)(msg, &mut it) == 0
            || (dbus.dbus_message_iter_get_arg_type)(&mut it) != DBUS_TYPE_ARRAY
        {
            return out;
        }
        let mut ait: DBusMessageIter = std::mem::zeroed();
        (dbus.dbus_message_iter_recurse)(&mut it, &mut ait);
        while (dbus.dbus_message_iter_get_arg_type)(&mut ait) == DBUS_TYPE_STRING {
            let mut p: *const c_char = std::ptr::null();
            (dbus.dbus_message_iter_get_basic)(&mut ait, &mut p as *mut *const c_char as *mut c_void);
            if !p.is_null() {
                out.push(CStr::from_ptr(p).to_string_lossy().into_owned());
            }
            if (dbus.dbus_message_iter_next)(&mut ait) == 0 {
                break;
            }
        }
    }
    out
}

/// A UINT32 then a second UINT32 or STRING - the two signals' arguments.
unsafe fn read_u32_then(dbus: &DBusLib, msg: *mut DBusMessage) -> Option<(u32, SecondArg)> {
    unsafe {
        let mut it: DBusMessageIter = std::mem::zeroed();
        if (dbus.dbus_message_iter_init)(msg, &mut it) == 0
            || (dbus.dbus_message_iter_get_arg_type)(&mut it) != DBUS_TYPE_UINT32
        {
            return None;
        }
        let mut id: u32 = 0;
        (dbus.dbus_message_iter_get_basic)(&mut it, &mut id as *mut u32 as *mut c_void);
        if (dbus.dbus_message_iter_next)(&mut it) == 0 {
            return None;
        }
        match (dbus.dbus_message_iter_get_arg_type)(&mut it) {
            DBUS_TYPE_UINT32 => {
                let mut v: u32 = 0;
                (dbus.dbus_message_iter_get_basic)(&mut it, &mut v as *mut u32 as *mut c_void);
                Some((id, SecondArg::U32(v)))
            }
            DBUS_TYPE_STRING => {
                let mut p: *const c_char = std::ptr::null();
                (dbus.dbus_message_iter_get_basic)(&mut it, &mut p as *mut *const c_char as *mut c_void);
                if p.is_null() {
                    return None;
                }
                Some((
                    id,
                    SecondArg::Str(CStr::from_ptr(p).to_string_lossy().into_owned()),
                ))
            }
            _ => None,
        }
    }
}

enum SecondArg {
    U32(u32),
    Str(String),
}

/// The app id a server id belongs to; `remove` forgets the mapping (the
/// notification closed, so the server will not report it again).
fn app_id_of(server_id: u32, remove: bool) -> Option<String> {
    let mut ids = SERVER_IDS.lock().unwrap_or_else(PoisonError::into_inner);
    if remove {
        ids.remove(&server_id)
    } else {
        ids.get(&server_id).cloned()
    }
}

/// The server id currently showing the app's notification `app_id`.
fn server_id_of(app_id: &str) -> Option<u32> {
    let ids = SERVER_IDS.lock().unwrap_or_else(PoisonError::into_inner);
    ids.iter()
        .find(|(_, a)| a.as_str() == app_id)
        .map(|(server, _)| *server)
}

/// Sees every message the shared connection dispatches; acts on the two
/// notification signals and never claims anything.
unsafe extern "C" fn notification_filter(
    _conn: *mut DBusConnection,
    msg: *mut DBusMessage,
    _user_data: *mut c_void,
) -> c_int {
    let Some(dbus) = get_shared_dbus_lib() else {
        return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
    };
    unsafe {
        let iface = (dbus.dbus_message_get_interface)(msg);
        let member = (dbus.dbus_message_get_member)(msg);
        if iface.is_null() || member.is_null() || CStr::from_ptr(iface).to_bytes() != IFACE.as_bytes()
        {
            return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
        }
        match CStr::from_ptr(member).to_bytes() {
            b"ActionInvoked" => {
                if let Some((server_id, SecondArg::Str(key))) = read_u32_then(&dbus, msg) {
                    if let Some(app_id) = app_id_of(server_id, false) {
                        queue_notification_event(wire::freedesktop_action_event(&app_id, &key));
                    }
                }
            }
            b"ActivationToken" => {
                if let Some((server_id, SecondArg::Str(token))) = read_u32_then(&dbus, msg) {
                    if app_id_of(server_id, false).is_some() && !token.is_empty() {
                        *ACTIVATION_TOKEN
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner) = Some(token);
                    }
                }
            }
            b"NotificationClosed" => {
                if let Some((server_id, SecondArg::U32(reason))) = read_u32_then(&dbus, msg) {
                    if let Some(app_id) = app_id_of(server_id, true) {
                        queue_notification_event(wire::freedesktop_closed_event(&app_id, reason));
                    }
                }
            }
            _ => {}
        }
    }
    DBUS_HANDLER_RESULT_NOT_YET_HANDLED
}

/// `GetServerInformation` -> "name version", or why there is no server.
unsafe fn server_information(
    dbus: &DBusLib,
    conn: *mut DBusConnection,
    timeout_ms: c_int,
) -> Result<String, String> {
    unsafe {
        let msg = method_call(dbus, "GetServerInformation")?;
        let reply = call(dbus, conn, msg, timeout_ms, "GetServerInformation").map_err(|e| {
            format!("no notification server on the session bus ({DEST}): {e}")
        })?;
        let strings = read_strings(dbus, reply);
        (dbus.dbus_message_unref)(reply);
        // (name, vendor, version, spec_version)
        Ok(match (strings.first(), strings.get(2)) {
            (Some(name), Some(version)) => format!("{name} {version}"),
            (Some(name), None) => name.clone(),
            _ => "unknown server".to_string(),
        })
    }
}

/// `GetCapabilities` (`"actions"`, `"body"`, `"sound"`, ...).
unsafe fn capabilities(dbus: &DBusLib, conn: *mut DBusConnection, timeout_ms: c_int) -> Vec<String> {
    unsafe {
        let Ok(msg) = method_call(dbus, "GetCapabilities") else {
            return Vec::new();
        };
        let Ok(reply) = call(dbus, conn, msg, timeout_ms, "GetCapabilities") else {
            return Vec::new();
        };
        let caps = read_string_array(dbus, reply);
        (dbus.dbus_message_unref)(reply);
        caps
    }
}

/// `(available, backend, reason)`.
type Probe = (bool, String, String);

/// `(available, backend, reason)` for `PlatformCapability::notifications()`.
/// Cached for ten seconds: it is two D-Bus round trips, and an app may well
/// ask from a layout callback.
pub(super) fn probe() -> Probe {
    static CACHE: Mutex<Option<(Instant, Probe)>> = Mutex::new(None);
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((at, value)) = cache.as_ref() {
        if at.elapsed() < Duration::from_secs(10) {
            return value.clone();
        }
    }
    let value = probe_uncached();
    *cache = Some((Instant::now(), value.clone()));
    value
}

fn probe_uncached() -> Probe {
    let backend = format!("{DEST} (D-Bus)");
    let Some(dbus) = get_shared_dbus_lib() else {
        return (false, backend, "libdbus-1.so.3 could not be loaded".to_string());
    };
    unsafe {
        let conn = match session(&dbus) {
            Ok(c) => c,
            Err(e) => return (false, backend, e),
        };
        let result = match server_information(&dbus, conn, PROBE_TIMEOUT_MS) {
            Err(e) => (false, backend, e),
            Ok(server) => {
                let caps = capabilities(&dbus, conn, PROBE_TIMEOUT_MS);
                let backend = format!("{DEST} ({server})");
                if caps.iter().any(|c| c == "actions") {
                    (true, backend, String::new())
                } else {
                    (
                        true,
                        backend,
                        "the notification server shows no buttons and reports no clicks (it does \
                         not advertise the `actions` capability); dismissals are still reported"
                            .to_string(),
                    )
                }
            }
        };
        // `dbus_bus_get` hands back the shared connection with a reference
        // added for us - balance it, as `native_global_menu_available` does.
        (dbus.dbus_connection_unref)(conn);
        result
    }
}


pub(super) struct PlatformNotifier {
    dbus: Arc<DBusLib>,
    conn: *mut DBusConnection,
    app_name: String,
    /// The `desktop-entry` hint (module docs).
    desktop_entry: String,
}

impl PlatformNotifier {
    pub(super) fn new() -> Result<Self, String> {
        let dbus = get_shared_dbus_lib().ok_or("libdbus-1.so.3 could not be loaded")?;
        unsafe {
            let conn = session(&dbus)?;
            // Is anybody serving notifications? (Activates an installed daemon.)
            let server = server_information(&dbus, conn, NOTIFY_TIMEOUT_MS)?;
            let caps = capabilities(&dbus, conn, NOTIFY_TIMEOUT_MS);

            // Route both signals to this connection (GNOME broadcasts them),
            // then watch for them as the connection is dispatched.
            let rule = CString::new(format!("type='signal',interface='{IFACE}'")).unwrap_or_default();
            let mut err = fresh_error();
            (dbus.dbus_error_init)(&mut err);
            (dbus.dbus_bus_add_match)(conn, rule.as_ptr(), &mut err);
            if (dbus.dbus_error_is_set)(&err) != 0 {
                return Err(take_error(
                    &dbus,
                    &mut err,
                    "could not subscribe to the notification signals",
                ));
            }
            (dbus.dbus_error_free)(&mut err);
            if (dbus.dbus_connection_add_filter)(
                conn,
                Some(notification_filter),
                std::ptr::null_mut(),
                None,
            ) == 0
            {
                return Err("dbus_connection_add_filter failed".to_string());
            }
            crate::plog_info!(
                "[notifications] freedesktop notification server: {server}; capabilities: \
                 {caps:?}"
            );
            if !caps.iter().any(|c| c == "actions") {
                crate::plog_warn!(
                    "[notifications] the server does not advertise `actions`: notification \
                     buttons are not shown and clicks are not reported"
                );
            }
            // ActionInvoked / NotificationClosed arrive on this socket: put it
            // in the run loops' wait set (the tray's, when both exist - it is
            // the same shared connection).
            crate::desktop::loop_waker::watch_dbus_connection(&dbus, conn);
            // The app's one identity: `app_name` is what servers group and
            // label by; the hint is the same string as the Wayland `app_id`.
            let app = crate::desktop::app_identity::current();
            Ok(Self {
                dbus,
                conn,
                app_name: app.display_name(),
                desktop_entry: app.desktop_entry(),
            })
        }
    }

    pub(super) fn post(&mut self, notification: &Notification) -> Result<(), String> {
        let app_id = notification.id.as_str().to_string();
        let replaces = server_id_of(&app_id).unwrap_or(0);
        let dbus = self.dbus.clone();
        let server_id = unsafe {
            let msg = method_call(&dbus, "Notify")?;
            let mut it: DBusMessageIter = std::mem::zeroed();
            (dbus.dbus_message_iter_init_append)(msg, &mut it);
            append_str(&dbus, &mut it, &self.app_name);
            append_u32(&dbus, &mut it, replaces);
            let icon = notification
                .icon
                .as_ref()
                .map(|i| wire::freedesktop_icon(i.as_str()))
                .unwrap_or_default();
            append_str(&dbus, &mut it, &icon);
            append_str(&dbus, &mut it, notification.title.as_str());
            append_str(&dbus, &mut it, notification.body.as_str());

            // actions: as
            let s_sig = CString::new("s").unwrap_or_default();
            let mut actions: DBusMessageIter = std::mem::zeroed();
            (dbus.dbus_message_iter_open_container)(
                &mut it,
                DBUS_TYPE_ARRAY,
                s_sig.as_ptr(),
                &mut actions,
            );
            for entry in wire::freedesktop_actions(notification) {
                append_str(&dbus, &mut actions, &entry);
            }
            (dbus.dbus_message_iter_close_container)(&mut it, &mut actions);

            // hints: a{sv}
            let dict_sig = CString::new("{sv}").unwrap_or_default();
            let mut hints: DBusMessageIter = std::mem::zeroed();
            (dbus.dbus_message_iter_open_container)(
                &mut it,
                DBUS_TYPE_ARRAY,
                dict_sig.as_ptr(),
                &mut hints,
            );
            append_hint(
                &dbus,
                &mut hints,
                "desktop-entry",
                Hint::Str(&self.desktop_entry),
            );
            match &notification.sound {
                NotificationSound::Default => {}
                NotificationSound::Silent => {
                    append_hint(&dbus, &mut hints, "suppress-sound", Hint::Bool(true));
                }
                NotificationSound::Named(name) => {
                    append_hint(&dbus, &mut hints, "sound-name", Hint::Str(name.as_str()));
                }
            }
            (dbus.dbus_message_iter_close_container)(&mut it, &mut hints);

            // expire_timeout: -1 = the server's default.
            append_i32(&dbus, &mut it, -1);

            let reply = call(&dbus, self.conn, msg, NOTIFY_TIMEOUT_MS, "Notify")?;
            let mut rit: DBusMessageIter = std::mem::zeroed();
            let mut id: u32 = 0;
            if (dbus.dbus_message_iter_init)(reply, &mut rit) != 0
                && (dbus.dbus_message_iter_get_arg_type)(&mut rit) == DBUS_TYPE_UINT32
            {
                (dbus.dbus_message_iter_get_basic)(&mut rit, &mut id as *mut u32 as *mut c_void);
            }
            (dbus.dbus_message_unref)(reply);
            id
        };
        if server_id == 0 {
            return Err("the notification server answered Notify without an id".to_string());
        }
        let mut ids = SERVER_IDS.lock().unwrap_or_else(PoisonError::into_inner);
        ids.retain(|_, a| a.as_str() != app_id.as_str());
        ids.insert(server_id, app_id);
        Ok(())
    }

    pub(super) fn withdraw(&mut self, id: &str) {
        let Some(server_id) = server_id_of(id) else {
            return;
        };
        // Forget first: the NotificationClosed(reason 3) that confirms this
        // must not be reported as a dismissal.
        let _ = app_id_of(server_id, true);
        unsafe {
            let Ok(msg) = method_call(&self.dbus, "CloseNotification") else {
                return;
            };
            let mut it: DBusMessageIter = std::mem::zeroed();
            (self.dbus.dbus_message_iter_init_append)(msg, &mut it);
            append_u32(&self.dbus, &mut it, server_id);
            (self.dbus.dbus_connection_send)(self.conn, msg, std::ptr::null_mut());
            (self.dbus.dbus_message_unref)(msg);
            (self.dbus.dbus_connection_flush)(self.conn);
        }
    }

    /// Dispatch what arrived on the connection, which runs the filter. The
    /// shared drain reads once and dispatches until libdbus's queue is empty
    /// (`read_write_dispatch` did one message per call, so a burst larger
    /// than its old 8-call budget waited for the next wake-up).
    pub(super) fn pump(&mut self) {
        unsafe {
            crate::desktop::shell2::linux::dbus::drain_connection(&self.dbus, self.conn);
        }
    }
}
