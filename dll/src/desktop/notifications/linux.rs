//! Linux native notifications  -  `org.freedesktop.Notifications` over D-Bus,
//! or the Flatpak portal `org.freedesktop.portal.Notification`.
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
//!   expire_timeout i) -> u`: the SERVER picks the id. [`POSTS`] (`wire::FreedesktopPosts`) maps
//!   it back to the app's string id, and `replaces_id` re-uses it when the app posts the same id
//!   again.
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
//! # `Notify` does not block
//!
//! A post SENDS `Notify` with a pending call and returns; the reply (the server's id) completes
//! the call when the connection is dispatched - by the run loop's drain, whose wait set has the
//! D-Bus socket - and [`PlatformNotifier::pump`] collects it. Until then the app may post again
//! under the same id or withdraw it; `wire::FreedesktopPosts` sorts that out (a stale reply is
//! closed, a withdrawn one too). An error reply, or none within [`NOTIFY_REPLY_TIMEOUT_MS`], is a
//! `Failed` event. (Only the backend's START - `GetServerInformation` at the first post - still
//! waits for the server, once.)
//!
//! # The server restarts
//!
//! A match on `NameOwnerChanged` for `org.freedesktop.Notifications` tells when the server leaves
//! the bus (it crashed, quit, or was replaced). Its notifications went with it: each one this app
//! had on screen ends as `Dismissed`, and the next post starts on the new server with nothing to
//! replace.
//!
//! # Identity
//!
//! Every `Notify` carries the `desktop-entry` hint and the `app_name` of the app's one identity
//! (`desktop::app_identity`): the executable's name - the same string as the Wayland `app_id` and
//! the X11 `WM_CLASS` default. GNOME matches a sender by its window's PID first; a windowless or
//! tray-only process is matched by this hint, and without a match its notifications get a generic
//! source with no per-app settings.
//!
//! # Inside Flatpak: the portal
//!
//! A Flatpak sandbox (`wire::in_flatpak_sandbox`) filters the session bus down to the portals, so
//! the backend talks to `org.freedesktop.portal.Notification` on `org.freedesktop.portal.Desktop`
//! instead, over the same connection: `AddNotification(id, a{sv})` keyed by the app's OWN id (no
//! server id to map; the portal attributes it to the sandbox's app id), `RemoveNotification(id)`,
//! and the signal `ActionInvoked(id, action, av)`. The portal has no closed signal, so a
//! dismissal is never reported there. Unsandboxed, the portal is not used: GNOME's portal backend
//! drops the notifications of an app id that has no `.desktop` file.
//!
//! # Receiving the signals
//!
//! GNOME Shell BROADCASTS the notification signals, dunst sends them to the
//! caller only; a match rule (`dbus_bus_add_match`) makes the bus route them
//! to this connection, and a filter (`dbus_connection_add_filter`) sees them
//! as the connection is dispatched - by [`PlatformNotifier::pump`], and by the
//! tray's pump. Signals for other applications' notifications carry ids that
//! are not in [`POSTS`] and are ignored. The filter never claims a message,
//! so the tray and the GNOME menu handlers on the same connection see
//! everything they saw before.
//!
//! # No server
//!
//! A session without a notification daemon (a bare window manager with no
//! dunst/mako) has no `org.freedesktop.Notifications`, and D-Bus activation
//! cannot start one. `GetServerInformation` fails, the backend is unavailable
//! with that reason, and posts become `Failed` events - never a silent no-op.

use std::{
    ffi::{CStr, CString},
    os::raw::{c_char, c_int, c_uint, c_void},
    sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError},
    time::{Duration, Instant},
};

use azul_core::notification::{Notification, NotificationEvent, NotificationSound};
use azul_css::AzString;
use azul_layout::managers::notification::{
    queue_notification_event,
    wire::{self, FreedesktopActions, FreedesktopPosts},
};

use crate::desktop::shell2::linux::{
    dbus::{
        DBusConnection, DBusError, DBusLib, DBusMessage, DBusMessageIter, DBusPendingCall,
        DBUS_BUS_SESSION, DBUS_HANDLER_RESULT_NOT_YET_HANDLED, DBUS_MESSAGE_TYPE_ERROR,
        DBUS_TYPE_ARRAY, DBUS_TYPE_BOOLEAN, DBUS_TYPE_DICT_ENTRY, DBUS_TYPE_INT32,
        DBUS_TYPE_STRING, DBUS_TYPE_UINT32, DBUS_TYPE_VARIANT,
    },
    gnome_menu::get_shared_dbus_lib,
};

const DEST: &str = wire::FREEDESKTOP_SERVER_NAME;
const PATH: &str = "/org/freedesktop/Notifications";
const IFACE: &str = "org.freedesktop.Notifications";
/// The portal: its bus name, object and interface.
const PORTAL_DEST: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const PORTAL_IFACE: &str = "org.freedesktop.portal.Notification";
/// The bus itself, which sends `NameOwnerChanged`.
const BUS_IFACE: &str = "org.freedesktop.DBus";
/// The backend's START waits for the server once (`GetServerInformation`):
/// D-Bus activation of a daemon that is installed but not running can take a
/// moment.
const START_TIMEOUT_MS: c_int = 3000;
/// The capability probe must not stall a layout callback that asks.
const PROBE_TIMEOUT_MS: c_int = 1000;
/// How long a `Notify` (or `AddNotification`) may wait for its reply before
/// it counts as failed - the D-Bus default. Checked when the loop comes
/// around; nothing wakes it for this alone.
const NOTIFY_REPLY_TIMEOUT_MS: u64 = 25_000;

/// Server id <-> the app's id, and the posts whose reply is outstanding.
/// Written by `post` / `withdraw` / `pump`, read by the filter (which runs
/// inside a dispatch and has no other state) - all on the event-loop thread.
/// Never held across a dispatch: the filter locks it.
static POSTS: Mutex<FreedesktopPosts> = Mutex::new(FreedesktopPosts::new());

/// The `ActivationToken` of the last click on one of this app's
/// notifications, until the run loop takes it.
static ACTIVATION_TOKEN: Mutex<Option<String>> = Mutex::new(None);

/// The capability probe's cache (it is two D-Bus round trips). Cleared when
/// the server leaves the bus, so the next probe asks the new one.
static PROBE_CACHE: Mutex<Option<(Instant, Probe)>> = Mutex::new(None);

fn posts() -> MutexGuard<'static, FreedesktopPosts> {
    POSTS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Milliseconds since the backend's first use: the clock of
/// `wire::FreedesktopPosts::expired`.
fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    let start = *START.get_or_init(Instant::now);
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Take the activation token a click brought (see the module docs).
pub(super) fn take_activation_token() -> Option<String> {
    ACTIVATION_TOKEN
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
}

/// Does this process run in a Flatpak sandbox? Then the portal is the
/// transport (module docs).
fn sandboxed() -> bool {
    let flatpak_id = std::env::var("FLATPAK_ID").ok();
    wire::in_flatpak_sandbox(
        std::path::Path::new("/.flatpak-info").exists(),
        flatpak_id.as_deref(),
    )
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

/// Add a match rule; `Err` with the bus's reason.
unsafe fn add_match(dbus: &DBusLib, conn: *mut DBusConnection, rule: &str) -> Result<(), String> {
    let rule = CString::new(rule).unwrap_or_default();
    let mut err = fresh_error();
    unsafe {
        (dbus.dbus_error_init)(&mut err);
        (dbus.dbus_bus_add_match)(conn, rule.as_ptr(), &mut err);
        if (dbus.dbus_error_is_set)(&err) != 0 {
            return Err(take_error(dbus, &mut err, "the bus refused a match rule"));
        }
        (dbus.dbus_error_free)(&mut err);
    }
    Ok(())
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

/// One `{sv}` entry of a dictionary, the value a STRING or BOOLEAN.
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

/// A method call: `(destination, path, interface)` and the member.
unsafe fn method_call(
    dbus: &DBusLib,
    (dest, path, iface): (&str, &str, &str),
    member: &str,
) -> Result<*mut DBusMessage, String> {
    let dest = CString::new(dest).unwrap_or_default();
    let path = CString::new(path).unwrap_or_default();
    let iface = CString::new(iface).unwrap_or_default();
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

/// The notification server's object.
const SERVER: (&str, &str, &str) = (DEST, PATH, IFACE);
/// The portal's object.
const PORTAL: (&str, &str, &str) = (PORTAL_DEST, PORTAL_PATH, PORTAL_IFACE);

/// Send `msg` (consumed) and wait for the reply, which the caller unrefs.
/// Only the backend's start and the capability probe wait like this.
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

/// Send `msg` (consumed) WITHOUT waiting: the reply completes the returned
/// pending call when the connection is dispatched.
unsafe fn send_async(
    dbus: &DBusLib,
    conn: *mut DBusConnection,
    msg: *mut DBusMessage,
    what: &str,
) -> Result<*mut DBusPendingCall, String> {
    let mut pending: *mut DBusPendingCall = std::ptr::null_mut();
    let timeout = c_int::try_from(NOTIFY_REPLY_TIMEOUT_MS).unwrap_or(c_int::MAX);
    let sent = unsafe { (dbus.dbus_connection_send_with_reply)(conn, msg, &mut pending, timeout) };
    unsafe { (dbus.dbus_message_unref)(msg) };
    if sent == 0 || pending.is_null() {
        // libdbus is out of memory, or the connection is closed (then it
        // hands back a NULL pending call).
        return Err(format!(
            "{what} could not be sent: the session bus connection is closed"
        ));
    }
    unsafe { (dbus.dbus_connection_flush)(conn) };
    Ok(pending)
}

/// The outcome of a completed pending call: the reply's first `UINT32` (0
/// when there is none - the portal's replies carry nothing), or the error
/// an error reply carries. Consumes the pending call and its reply.
unsafe fn take_reply(dbus: &DBusLib, pending: *mut DBusPendingCall) -> Result<u32, String> {
    unsafe {
        let reply = (dbus.dbus_pending_call_steal_reply)(pending);
        (dbus.dbus_pending_call_unref)(pending);
        if reply.is_null() {
            return Err("the call completed without a reply".to_string());
        }
        let result = if (dbus.dbus_message_get_type)(reply) == DBUS_MESSAGE_TYPE_ERROR {
            let mut err = fresh_error();
            (dbus.dbus_error_init)(&mut err);
            (dbus.dbus_set_error_from_message)(&mut err, reply);
            Err(take_error(dbus, &mut err, "the call was refused"))
        } else {
            let mut it: DBusMessageIter = std::mem::zeroed();
            let mut id: u32 = 0;
            if (dbus.dbus_message_iter_init)(reply, &mut it) != 0
                && (dbus.dbus_message_iter_get_arg_type)(&mut it) == DBUS_TYPE_UINT32
            {
                (dbus.dbus_message_iter_get_basic)(&mut it, &mut id as *mut u32 as *mut c_void);
            }
            Ok(id)
        };
        (dbus.dbus_message_unref)(reply);
        result
    }
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

/// A UINT32 then a second UINT32 or STRING - the server signals' arguments.
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

/// Sees every message the shared connection dispatches; acts on the
/// notification signals (the server's, the portal's) and on the server
/// leaving the bus, and never claims anything.
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
        if iface.is_null() || member.is_null() {
            return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
        }
        let iface = CStr::from_ptr(iface).to_bytes();
        let member = CStr::from_ptr(member).to_bytes();
        if iface == IFACE.as_bytes() {
            server_signal(&dbus, msg, member);
        } else if iface == PORTAL_IFACE.as_bytes() && member == b"ActionInvoked" {
            // (s id, s action, av parameter): the app's own id, no mapping.
            let strings = read_strings(&dbus, msg);
            if let (Some(id), Some(action)) = (strings.first(), strings.get(1)) {
                queue_notification_event(wire::portal_action_event(id, action));
            }
        } else if iface == BUS_IFACE.as_bytes() && member == b"NameOwnerChanged" {
            // (s name, s old_owner, s new_owner)
            let strings = read_strings(&dbus, msg);
            if let (Some(name), Some(old), Some(new)) =
                (strings.first(), strings.get(1), strings.get(2))
            {
                if wire::freedesktop_server_left(name, old, new) {
                    server_left();
                }
            }
        }
    }
    DBUS_HANDLER_RESULT_NOT_YET_HANDLED
}

/// `ActionInvoked` / `ActivationToken` / `NotificationClosed` from the server.
unsafe fn server_signal(dbus: &DBusLib, msg: *mut DBusMessage, member: &[u8]) {
    match member {
        b"ActionInvoked" => {
            if let Some((server_id, SecondArg::Str(key))) = unsafe { read_u32_then(dbus, msg) } {
                let app_id = posts().app_id_of(server_id);
                if let Some(app_id) = app_id {
                    queue_notification_event(wire::freedesktop_action_event(&app_id, &key));
                }
            }
        }
        b"ActivationToken" => {
            if let Some((server_id, SecondArg::Str(token))) = unsafe { read_u32_then(dbus, msg) } {
                let ours = posts().app_id_of(server_id).is_some();
                if ours && !token.is_empty() {
                    *ACTIVATION_TOKEN
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = Some(token);
                }
            }
        }
        b"NotificationClosed" => {
            if let Some((server_id, SecondArg::U32(reason))) = unsafe { read_u32_then(dbus, msg) } {
                let app_id = posts().closed(server_id);
                if let Some(app_id) = app_id {
                    queue_notification_event(wire::freedesktop_closed_event(&app_id, reason));
                }
            }
        }
        _ => {}
    }
}

/// The notification server left the bus: everything it showed is gone.
fn server_left() {
    let gone = posts().server_gone();
    if !gone.events.is_empty() {
        crate::plog_warn!(
            "[notifications] the notification server left the session bus; {} notification(s) \
             it showed are gone",
            gone.events.len()
        );
    }
    for event in gone.events {
        queue_notification_event(event);
    }
    *PROBE_CACHE.lock().unwrap_or_else(PoisonError::into_inner) = None;
}

/// `GetServerInformation` -> "name version", or why there is no server.
unsafe fn server_information(
    dbus: &DBusLib,
    conn: *mut DBusConnection,
    timeout_ms: c_int,
) -> Result<String, String> {
    unsafe {
        let msg = method_call(dbus, SERVER, "GetServerInformation")?;
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
        let Ok(msg) = method_call(dbus, SERVER, "GetCapabilities") else {
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
    let mut cache = PROBE_CACHE.lock().unwrap_or_else(PoisonError::into_inner);
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
    if sandboxed() {
        return (
            true,
            format!("{PORTAL_IFACE} (Flatpak)"),
            "inside a Flatpak sandbox notifications go through the portal, which reports clicks \
             and buttons but never a dismissal (it has no closed signal)"
                .to_string(),
        );
    }
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

/// Where the notifications go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Transport {
    /// `org.freedesktop.Notifications`, unsandboxed.
    Server,
    /// `org.freedesktop.portal.Notification`, inside Flatpak.
    Portal,
}

/// An `AddNotification` whose reply has not arrived. The portal keys
/// notifications by the app's own id and replaces by it, so there is nothing
/// to map; only a failure is news - and only the newest post's.
struct PortalPost {
    app_id: String,
    pending: *mut DBusPendingCall,
    sent_at_ms: u64,
}

pub(super) struct PlatformNotifier {
    dbus: Arc<DBusLib>,
    conn: *mut DBusConnection,
    transport: Transport,
    app_name: String,
    /// The `desktop-entry` hint (module docs).
    desktop_entry: String,
    /// `Notify` calls whose reply has not arrived: `(POSTS token, call)`.
    in_flight: Vec<(u64, *mut DBusPendingCall)>,
    /// `AddNotification` calls whose reply has not arrived, oldest first.
    portal_in_flight: Vec<PortalPost>,
}

impl PlatformNotifier {
    pub(super) fn new() -> Result<Self, String> {
        let dbus = get_shared_dbus_lib().ok_or("libdbus-1.so.3 could not be loaded")?;
        let transport = if sandboxed() {
            Transport::Portal
        } else {
            Transport::Server
        };
        unsafe {
            let conn = session(&dbus)?;
            match transport {
                Transport::Server => {
                    // Is anybody serving notifications? (Activates an
                    // installed daemon.) The one call that still waits.
                    let server = server_information(&dbus, conn, START_TIMEOUT_MS)?;
                    let caps = capabilities(&dbus, conn, START_TIMEOUT_MS);
                    // Route the server's signals to this connection (GNOME
                    // broadcasts them), and tell when the server leaves.
                    add_match(&dbus, conn, &format!("type='signal',interface='{IFACE}'"))?;
                    if let Err(e) = add_match(
                        &dbus,
                        conn,
                        &format!(
                            "type='signal',sender='{BUS_IFACE}',interface='{BUS_IFACE}',\
                             member='NameOwnerChanged',arg0='{DEST}'"
                        ),
                    ) {
                        crate::plog_warn!(
                            "[notifications] a restart of the notification server will go \
                             unnoticed: {e}"
                        );
                    }
                    crate::plog_info!(
                        "[notifications] freedesktop notification server: {server}; \
                         capabilities: {caps:?}"
                    );
                    if !caps.iter().any(|c| c == "actions") {
                        crate::plog_warn!(
                            "[notifications] the server does not advertise `actions`: \
                             notification buttons are not shown and clicks are not reported"
                        );
                    }
                }
                Transport::Portal => {
                    // The portal's `ActionInvoked` is sent to this app alone;
                    // the rule is for a proxy that only forwards matched ones.
                    add_match(
                        &dbus,
                        conn,
                        &format!("type='signal',interface='{PORTAL_IFACE}'"),
                    )?;
                    crate::plog_info!(
                        "[notifications] inside a Flatpak sandbox: notifications go through \
                         {PORTAL_IFACE}"
                    );
                }
            }
            if (dbus.dbus_connection_add_filter)(
                conn,
                Some(notification_filter),
                std::ptr::null_mut(),
                None,
            ) == 0
            {
                return Err("dbus_connection_add_filter failed".to_string());
            }
            // The signals and the replies arrive on this socket: put it in
            // the run loops' wait set (the tray's, when both exist - it is
            // the same shared connection).
            crate::desktop::loop_waker::watch_dbus_connection(&dbus, conn);
            // The app's one identity: `app_name` is what servers group and
            // label by; the hint is the same string as the Wayland `app_id`.
            let app = crate::desktop::app_identity::current();
            Ok(Self {
                dbus,
                conn,
                transport,
                app_name: app.display_name(),
                desktop_entry: app.desktop_entry(),
                in_flight: Vec::new(),
                portal_in_flight: Vec::new(),
            })
        }
    }

    pub(super) fn post(&mut self, notification: &Notification) -> Result<(), String> {
        match self.transport {
            Transport::Server => self.post_to_server(notification),
            Transport::Portal => self.post_to_portal(notification),
        }
    }

    /// `Notify`, sent without waiting; [`PlatformNotifier::pump`] collects
    /// the server's id.
    fn post_to_server(&mut self, notification: &Notification) -> Result<(), String> {
        let app_id = notification.id.as_str().to_string();
        let (token, replaces) = posts().post(&app_id, now_ms());
        let sent = unsafe { self.send_notify(notification, replaces) };
        match sent {
            Ok(pending) => {
                self.in_flight.push((token, pending));
                Ok(())
            }
            Err(e) => {
                // Never sent: forget the post. The service reports this
                // `Err` as the one `Failed` event.
                let _ = posts().replied(token, Err(e.clone()));
                Err(e)
            }
        }
    }

    unsafe fn send_notify(
        &self,
        notification: &Notification,
        replaces: u32,
    ) -> Result<*mut DBusPendingCall, String> {
        let dbus = &*self.dbus;
        unsafe {
            let msg = method_call(dbus, SERVER, "Notify")?;
            let mut it: DBusMessageIter = std::mem::zeroed();
            (dbus.dbus_message_iter_init_append)(msg, &mut it);
            append_str(dbus, &mut it, &self.app_name);
            append_u32(dbus, &mut it, replaces);
            let icon = notification
                .icon
                .as_ref()
                .map(|i| wire::freedesktop_icon(i.as_str()))
                .unwrap_or_default();
            append_str(dbus, &mut it, &icon);
            append_str(dbus, &mut it, notification.title.as_str());
            append_str(dbus, &mut it, notification.body.as_str());

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
                append_str(dbus, &mut actions, &entry);
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
                dbus,
                &mut hints,
                "desktop-entry",
                Hint::Str(&self.desktop_entry),
            );
            match &notification.sound {
                NotificationSound::Default => {}
                NotificationSound::Silent => {
                    append_hint(dbus, &mut hints, "suppress-sound", Hint::Bool(true));
                }
                NotificationSound::Named(name) => {
                    append_hint(dbus, &mut hints, "sound-name", Hint::Str(name.as_str()));
                }
            }
            (dbus.dbus_message_iter_close_container)(&mut it, &mut hints);

            // expire_timeout: -1 = the server's default.
            append_i32(dbus, &mut it, -1);

            send_async(dbus, self.conn, msg, "Notify")
        }
    }

    /// `AddNotification(id, a{sv})` on the portal, sent without waiting.
    fn post_to_portal(&mut self, notification: &Notification) -> Result<(), String> {
        let app_id = notification.id.as_str().to_string();
        let portal = wire::portal_notification(notification);
        let dbus = &*self.dbus;
        let pending = unsafe {
            let msg = method_call(dbus, PORTAL, "AddNotification")?;
            let mut it: DBusMessageIter = std::mem::zeroed();
            (dbus.dbus_message_iter_init_append)(msg, &mut it);
            append_str(dbus, &mut it, &app_id);

            let dict_sig = CString::new("{sv}").unwrap_or_default();
            let mut dict: DBusMessageIter = std::mem::zeroed();
            (dbus.dbus_message_iter_open_container)(
                &mut it,
                DBUS_TYPE_ARRAY,
                dict_sig.as_ptr(),
                &mut dict,
            );
            append_hint(dbus, &mut dict, "title", Hint::Str(&portal.title));
            if !portal.body.is_empty() {
                append_hint(dbus, &mut dict, "body", Hint::Str(&portal.body));
            }
            append_hint(
                dbus,
                &mut dict,
                "default-action",
                Hint::Str(&portal.default_action),
            );
            if !portal.buttons.is_empty() {
                // "buttons": <aa{sv}> - each button a {label, action} dict.
                let mut entry: DBusMessageIter = std::mem::zeroed();
                (dbus.dbus_message_iter_open_container)(
                    &mut dict,
                    DBUS_TYPE_DICT_ENTRY,
                    std::ptr::null(),
                    &mut entry,
                );
                append_str(dbus, &mut entry, "buttons");
                let variant_sig = CString::new("aa{sv}").unwrap_or_default();
                let mut variant: DBusMessageIter = std::mem::zeroed();
                (dbus.dbus_message_iter_open_container)(
                    &mut entry,
                    DBUS_TYPE_VARIANT,
                    variant_sig.as_ptr(),
                    &mut variant,
                );
                let button_sig = CString::new("a{sv}").unwrap_or_default();
                let mut list: DBusMessageIter = std::mem::zeroed();
                (dbus.dbus_message_iter_open_container)(
                    &mut variant,
                    DBUS_TYPE_ARRAY,
                    button_sig.as_ptr(),
                    &mut list,
                );
                for (label, action) in &portal.buttons {
                    let mut button: DBusMessageIter = std::mem::zeroed();
                    (dbus.dbus_message_iter_open_container)(
                        &mut list,
                        DBUS_TYPE_ARRAY,
                        dict_sig.as_ptr(),
                        &mut button,
                    );
                    append_hint(dbus, &mut button, "label", Hint::Str(label));
                    append_hint(dbus, &mut button, "action", Hint::Str(action));
                    (dbus.dbus_message_iter_close_container)(&mut list, &mut button);
                }
                (dbus.dbus_message_iter_close_container)(&mut variant, &mut list);
                (dbus.dbus_message_iter_close_container)(&mut entry, &mut variant);
                (dbus.dbus_message_iter_close_container)(&mut dict, &mut entry);
            }
            (dbus.dbus_message_iter_close_container)(&mut it, &mut dict);
            send_async(dbus, self.conn, msg, "AddNotification")?
        };
        self.portal_in_flight.push(PortalPost {
            app_id,
            pending,
            sent_at_ms: now_ms(),
        });
        Ok(())
    }

    pub(super) fn withdraw(&mut self, id: &str) {
        match self.transport {
            Transport::Server => {
                // Forget first: the NotificationClosed(reason 3) that
                // confirms this must not be reported as a dismissal. A post
                // still on its way is closed when its reply comes.
                let server_ids = posts().withdraw(id);
                for server_id in server_ids {
                    self.close_on_server(server_id);
                }
            }
            Transport::Portal => unsafe {
                let dbus = &*self.dbus;
                let Ok(msg) = method_call(dbus, PORTAL, "RemoveNotification") else {
                    return;
                };
                let mut it: DBusMessageIter = std::mem::zeroed();
                (dbus.dbus_message_iter_init_append)(msg, &mut it);
                append_str(dbus, &mut it, id);
                (dbus.dbus_connection_send)(self.conn, msg, std::ptr::null_mut());
                (dbus.dbus_message_unref)(msg);
                (dbus.dbus_connection_flush)(self.conn);
            },
        }
    }

    /// `CloseNotification(server_id)`, not waiting for anything.
    fn close_on_server(&self, server_id: u32) {
        let dbus = &*self.dbus;
        unsafe {
            let Ok(msg) = method_call(dbus, SERVER, "CloseNotification") else {
                return;
            };
            let mut it: DBusMessageIter = std::mem::zeroed();
            (dbus.dbus_message_iter_init_append)(msg, &mut it);
            append_u32(dbus, &mut it, server_id);
            (dbus.dbus_connection_send)(self.conn, msg, std::ptr::null_mut());
            (dbus.dbus_message_unref)(msg);
            (dbus.dbus_connection_flush)(self.conn);
        }
    }

    /// Dispatch what arrived on the connection, which runs the filter and
    /// completes the pending calls whose reply came; then collect those
    /// replies. The shared drain reads once and dispatches until libdbus's
    /// queue is empty (`read_write_dispatch` did one message per call, so a
    /// burst larger than its old 8-call budget waited for the next wake-up).
    pub(super) fn pump(&mut self) {
        unsafe {
            crate::desktop::shell2::linux::dbus::drain_connection(&self.dbus, self.conn);
        }
        self.collect_notify_replies();
        self.collect_portal_replies();
    }

    /// The `Notify` replies that came (or never will): the server's ids into
    /// `POSTS`, and what that asks for - stale notifications closed,
    /// failures reported.
    fn collect_notify_replies(&mut self) {
        if self.in_flight.is_empty() {
            return;
        }
        let expired = posts().expired(now_ms(), NOTIFY_REPLY_TIMEOUT_MS);
        let mut actions = FreedesktopActions::default();
        let mut waiting = Vec::with_capacity(self.in_flight.len());
        for (token, pending) in std::mem::take(&mut self.in_flight) {
            let result = if unsafe { (self.dbus.dbus_pending_call_get_completed)(pending) } != 0 {
                unsafe { take_reply(&self.dbus, pending) }
            } else if expired.contains(&token) {
                unsafe {
                    (self.dbus.dbus_pending_call_cancel)(pending);
                    (self.dbus.dbus_pending_call_unref)(pending);
                }
                Err(format!(
                    "the notification server did not answer Notify within {} s",
                    NOTIFY_REPLY_TIMEOUT_MS / 1000
                ))
            } else {
                waiting.push((token, pending));
                continue;
            };
            let outcome = posts().replied(token, result);
            actions.close.extend(outcome.close);
            actions.events.extend(outcome.events);
        }
        self.in_flight = waiting;
        for server_id in actions.close {
            self.close_on_server(server_id);
        }
        for event in actions.events {
            queue_notification_event(event);
        }
    }

    /// The `AddNotification` replies: only an error of the NEWEST post under
    /// an id is news (the portal replaces by id, so an older one's failure
    /// concerns nothing on screen).
    fn collect_portal_replies(&mut self) {
        if self.portal_in_flight.is_empty() {
            return;
        }
        let now = now_ms();
        let sent = std::mem::take(&mut self.portal_in_flight);
        let mut failed: Vec<(usize, String, String)> = Vec::new();
        let mut waiting = Vec::with_capacity(sent.len());
        for (index, post) in sent.into_iter().enumerate() {
            let completed = unsafe { (self.dbus.dbus_pending_call_get_completed)(post.pending) } != 0;
            let error = if completed {
                unsafe { take_reply(&self.dbus, post.pending) }.err()
            } else if now.saturating_sub(post.sent_at_ms) >= NOTIFY_REPLY_TIMEOUT_MS {
                unsafe {
                    (self.dbus.dbus_pending_call_cancel)(post.pending);
                    (self.dbus.dbus_pending_call_unref)(post.pending);
                }
                Some(format!(
                    "the notification portal did not answer within {} s",
                    NOTIFY_REPLY_TIMEOUT_MS / 1000
                ))
            } else {
                waiting.push((index, post));
                continue;
            };
            if let Some(why) = error {
                failed.push((index, post.app_id, why));
            }
        }
        for (index, app_id, why) in failed {
            let superseded = waiting
                .iter()
                .any(|(later, p)| *later > index && p.app_id == app_id);
            if !superseded {
                queue_notification_event(NotificationEvent::failed(
                    AzString::from(app_id),
                    AzString::from(why),
                ));
            }
        }
        self.portal_in_flight = waiting.into_iter().map(|(_, post)| post).collect();
    }
}
