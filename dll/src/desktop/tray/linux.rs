//! Linux system tray — `org.kde.StatusNotifierItem` over D-Bus.
//!
//! Implemented: item registration with the watcher, the full property set
//! (icon pixmaps in ARGB32 big-endian, tooltip, status, title), the
//! `Activate` / `SecondaryActivate` / `ContextMenu` / `Scroll` methods (posted
//! into the process-wide [`TrayEvent`] mailbox), `Introspectable`, an
//! `org.freedesktop.DBus.Properties` handler, and the `New*` signals on
//! update. Verified against xfce4-panel 4.18's StatusNotifier host.
//!
//! # Deliberately not here yet
//!
//! * **`com.canonical.dbusmenu`** — the panel-drawn context menu. SNI's `Menu`
//!   property points at a *dbusmenu* object: a recursive `(id, a{sv}, av)`
//!   tree served through `GetLayout`, which is a different protocol from the
//!   `org.gtk.Menus` scheme in `linux/gnome_menu/` (that one is roughly 30%
//!   reusable, and it is the boring 30%). Until it exists the `Menu` property
//!   answers the root path `/` — hosts that find no dbusmenu there fall back
//!   to calling `ContextMenu()`, which arrives as
//!   [`TrayEventType::ContextMenu`], so an app can still react.
//! * **Watcher restarts.** The panel restarting (plasmashell/waybar do this
//!   routinely) tears down the registration; re-registering needs a
//!   `NameOwnerChanged` match + filter, which needs `dbus_bus_add_match` /
//!   `dbus_connection_add_filter` in the dlopen wrapper. Until then a panel
//!   restart loses the icon until the app restarts.
//!
//! # The registration rules (measured conventions, not the published spec)
//!
//! * The bus name is `org.kde.StatusNotifierItem-<pid>-<n>` — **`org.kde.*`,
//!   not `org.freedesktop.*`**: the published fd.o text says the latter, but
//!   the reference implementation and the entire KDE stack use the former
//!   (Electron 43 broke Waybar by switching — Waybar#5240).
//! * `is_available()` checks that the watcher name is owned **AND** that
//!   `IsStatusNotifierHostRegistered` is true — a watcher with no host is a
//!   real state, because the watcher can win the startup race against the
//!   panel (cinnamon#13740). On a vanilla GNOME there is no watcher at all;
//!   that is not a bug to work around.
//! * Icons go as `IconPixmap` (`a(iiay)`, ARGB32 **big-endian** —
//!   [`azul_core::tray::TrayIconImage::to_argb32_be`] exists for exactly
//!   this). Named icons are rasterized through the icon registry instead of
//!   being passed as theme names the panel may not have.
//! * Hosts only re-read a property after the matching `New*` signal, which is
//!   why `update()` emits them (and why `dbus_message_new_signal` is in the
//!   dlopen wrapper).

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::sync::Arc;

use azul_core::tray::{
    TrayEvent, TrayEventType, TrayIconData, TrayIconSource, TrayScrollAxis, TrayStatus,
};

use super::{queue_tray_event, render_named_icon, TrayError};
use crate::desktop::shell2::linux::dbus::{
    DBusConnection, DBusError, DBusLib, DBusMessage, DBusMessageIter, DBusObjectPathVTable,
    DBUS_BUS_SESSION, DBUS_HANDLER_RESULT_HANDLED, DBUS_HANDLER_RESULT_NOT_YET_HANDLED,
    DBUS_TYPE_ARRAY, DBUS_TYPE_BOOLEAN, DBUS_TYPE_BYTE, DBUS_TYPE_DICT_ENTRY, DBUS_TYPE_INT32,
    DBUS_TYPE_OBJECT_PATH, DBUS_TYPE_STRING, DBUS_TYPE_STRUCT, DBUS_TYPE_UINT32, DBUS_TYPE_VARIANT,
};
use crate::desktop::shell2::linux::gnome_menu::get_shared_dbus_lib;

const SNI_IFACE: &str = "org.kde.StatusNotifierItem";
const SNI_PATH: &str = "/StatusNotifierItem";
const WATCHER_NAME: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const PROPS_IFACE: &str = "org.freedesktop.DBus.Properties";
const MENU_IFACE: &str = "com.canonical.dbusmenu";
const MENU_PATH: &str = "/MenuBar";
const INTROSPECT_IFACE: &str = "org.freedesktop.DBus.Introspectable";

/// Everything the property handler serves, plus the D-Bus handles it needs.
/// Boxed by [`PlatformTray`]; the box's address is the vtable user_data, so it
/// must stay put for the tray's lifetime (updates mutate through the box).
struct SniState {
    dbus: Arc<DBusLib>,
    id: String,
    title: String,
    tooltip: String,
    /// "Active" / "Passive" / "NeedsAttention".
    status: &'static str,
    /// Themed-icon fallback served as `IconName` when `pixmaps` is empty —
    /// an item with neither is invisible, which looks exactly like a bug.
    icon_name: String,
    /// `(width, height, ARGB32 big-endian bytes)`, largest last.
    pixmaps: Vec<(i32, i32, Vec<u8>)>,
    /// The dbusmenu tree, flattened: index = dbusmenu item id, `[0]` = the
    /// invisible root whose children are the top-level items. Length 1 (root
    /// only) = no menu; the `Menu` property then points at `/` so hosts fall
    /// back to `ContextMenu()`.
    menu: Vec<MenuNode>,
    /// dbusmenu layout revision, bumped by `update()`.
    menu_revision: u32,
    /// Item ids clicked by the host since the last `pump()`. A Mutex rather
    /// than a RefCell purely so the raw-pointer vtable handler and `pump`
    /// cannot alias UB-wise; both run on the main thread.
    clicked: std::sync::Mutex<Vec<i32>>,
}

/// One flattened dbusmenu item - see [`SniState::menu`].
struct MenuNode {
    /// Empty for a separator.
    label: String,
    separator: bool,
    enabled: bool,
    children: Vec<i32>,
    callback: Option<azul_core::menu::CoreMenuCallback>,
}

/// Flatten an azul [`Menu`](azul_core::menu::Menu) into the id-indexed
/// dbusmenu node list. `BreakLine` renders as a separator - dbusmenu has no
/// horizontal-flow concept.
fn flatten_menu(menu: Option<&azul_core::menu::Menu>) -> Vec<MenuNode> {
    use azul_core::menu::MenuItem;
    fn push_items(nodes: &mut Vec<MenuNode>, items: &[MenuItem], parent: usize) {
        for item in items {
            let id = nodes.len();
            match item {
                MenuItem::String(sm) => {
                    nodes.push(MenuNode {
                        label: sm.label.as_str().to_owned(),
                        separator: false,
                        enabled: !matches!(
                            sm.menu_item_state,
                            azul_core::menu::MenuItemState::Greyed
                                | azul_core::menu::MenuItemState::Disabled
                        ),
                        children: Vec::new(),
                        callback: sm.callback.as_ref().cloned(),
                    });
                    nodes[parent].children.push(id as i32);
                    let kids: &[MenuItem] = sm.children.as_ref();
                    if !kids.is_empty() {
                        push_items(nodes, kids, id);
                    }
                }
                MenuItem::Separator | MenuItem::BreakLine => {
                    nodes.push(MenuNode {
                        label: String::new(),
                        separator: true,
                        enabled: false,
                        children: Vec::new(),
                        callback: None,
                    });
                    nodes[parent].children.push(id as i32);
                }
            }
        }
    }
    let mut nodes = vec![MenuNode {
        label: String::new(),
        separator: false,
        enabled: true,
        children: Vec::new(),
        callback: None,
    }];
    if let Some(menu) = menu {
        push_items(&mut nodes, menu.items.as_ref(), 0);
    }
    nodes
}

pub(super) struct PlatformTray {
    dbus: Arc<DBusLib>,
    conn: *mut DBusConnection,
    /// Kept alive for the vtable's user_data pointer.
    state: Box<SniState>,
}

impl core::fmt::Debug for PlatformTray {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PlatformTray")
            .field("conn", &self.conn)
            .field("state", &self.state)
            .finish()
    }
}

impl core::fmt::Debug for SniState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SniState")
            .field("id", &self.id)
            .field("status", &self.status)
            .field("pixmaps", &self.pixmaps.len())
            .finish()
    }
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

/// Is a StatusNotifier host actually listening? Both halves matter — see the
/// module docs.
pub(super) fn is_available() -> bool {
    let Some(dbus) = get_shared_dbus_lib() else {
        return false;
    };
    unsafe {
        let mut err = fresh_error();
        (dbus.dbus_error_init)(&mut err);
        let conn = (dbus.dbus_bus_get)(DBUS_BUS_SESSION, &mut err);
        if conn.is_null() {
            (dbus.dbus_error_free)(&mut err);
            return false;
        }
        let watcher = CString::new(WATCHER_NAME).unwrap();
        let owned = (dbus.dbus_bus_name_has_owner)(conn, watcher.as_ptr(), &mut err) != 0;
        if (dbus.dbus_error_is_set)(&err) != 0 {
            (dbus.dbus_error_free)(&mut err);
            return false;
        }
        if !owned {
            return false;
        }
        watcher_host_registered(&dbus, conn).unwrap_or(false)
    }
}

/// `Properties.Get(org.kde.StatusNotifierWatcher, IsStatusNotifierHostRegistered)`.
unsafe fn watcher_host_registered(dbus: &DBusLib, conn: *mut DBusConnection) -> Option<bool> {
    let dest = CString::new(WATCHER_NAME).ok()?;
    let path = CString::new(WATCHER_PATH).ok()?;
    let iface = CString::new(PROPS_IFACE).ok()?;
    let member = CString::new("Get").ok()?;
    let msg = (dbus.dbus_message_new_method_call)(
        dest.as_ptr(),
        path.as_ptr(),
        iface.as_ptr(),
        member.as_ptr(),
    );
    if msg.is_null() {
        return None;
    }
    let mut it: DBusMessageIter = std::mem::zeroed();
    (dbus.dbus_message_iter_init_append)(msg, &mut it);
    append_str(dbus, &mut it, DBUS_TYPE_STRING, WATCHER_NAME);
    append_str(
        dbus,
        &mut it,
        DBUS_TYPE_STRING,
        "IsStatusNotifierHostRegistered",
    );
    let mut err = fresh_error();
    (dbus.dbus_error_init)(&mut err);
    let reply = (dbus.dbus_connection_send_with_reply_and_block)(conn, msg, 1000, &mut err);
    (dbus.dbus_message_unref)(msg);
    if reply.is_null() {
        (dbus.dbus_error_free)(&mut err);
        return None;
    }
    // Reply: VARIANT containing BOOLEAN.
    let mut rit: DBusMessageIter = std::mem::zeroed();
    let mut result = None;
    if (dbus.dbus_message_iter_init)(reply, &mut rit) != 0
        && (dbus.dbus_message_iter_get_arg_type)(&mut rit) == DBUS_TYPE_VARIANT
    {
        let mut vit: DBusMessageIter = std::mem::zeroed();
        (dbus.dbus_message_iter_recurse)(&mut rit, &mut vit);
        if (dbus.dbus_message_iter_get_arg_type)(&mut vit) == DBUS_TYPE_BOOLEAN {
            let mut b: c_uint = 0;
            (dbus.dbus_message_iter_get_basic)(&mut vit, &mut b as *mut c_uint as *mut c_void);
            result = Some(b != 0);
        }
    }
    (dbus.dbus_message_unref)(reply);
    result
}

// ---- marshalling helpers -----------------------------------------------------

/// Append one string-ish basic value (STRING or OBJECT_PATH).
unsafe fn append_str(dbus: &DBusLib, it: *mut DBusMessageIter, ty: c_int, s: &str) {
    let c = CString::new(s).unwrap_or_default();
    let p = c.as_ptr();
    (dbus.dbus_message_iter_append_basic)(it, ty, &p as *const *const c_char as *mut c_void);
}

unsafe fn append_i32(dbus: &DBusLib, it: *mut DBusMessageIter, v: i32) {
    (dbus.dbus_message_iter_append_basic)(it, DBUS_TYPE_INT32, &v as *const i32 as *mut c_void);
}

unsafe fn append_u32(dbus: &DBusLib, it: *mut DBusMessageIter, v: u32) {
    (dbus.dbus_message_iter_append_basic)(it, DBUS_TYPE_UINT32, &v as *const u32 as *const c_void);
}

unsafe fn append_bool(dbus: &DBusLib, it: *mut DBusMessageIter, v: bool) {
    let b: c_uint = if v { 1 } else { 0 };
    (dbus.dbus_message_iter_append_basic)(
        it,
        DBUS_TYPE_BOOLEAN,
        &b as *const c_uint as *mut c_void,
    );
}

/// Open a VARIANT of `sig`, run `f` inside it, close it.
unsafe fn in_variant(
    dbus: &DBusLib,
    it: *mut DBusMessageIter,
    sig: &str,
    f: impl FnOnce(&mut DBusMessageIter),
) {
    let sig_c = CString::new(sig).unwrap();
    let mut vit: DBusMessageIter = std::mem::zeroed();
    (dbus.dbus_message_iter_open_container)(it, DBUS_TYPE_VARIANT, sig_c.as_ptr(), &mut vit);
    f(&mut vit);
    (dbus.dbus_message_iter_close_container)(it, &mut vit);
}

/// The `a(iiay)` pixmap list.
unsafe fn append_pixmaps(dbus: &DBusLib, it: *mut DBusMessageIter, pix: &[(i32, i32, Vec<u8>)]) {
    let sig = CString::new("(iiay)").unwrap();
    let mut ait: DBusMessageIter = std::mem::zeroed();
    (dbus.dbus_message_iter_open_container)(it, DBUS_TYPE_ARRAY, sig.as_ptr(), &mut ait);
    for (w, h, bytes) in pix {
        let mut sit: DBusMessageIter = std::mem::zeroed();
        (dbus.dbus_message_iter_open_container)(
            &mut ait,
            DBUS_TYPE_STRUCT,
            std::ptr::null(),
            &mut sit,
        );
        append_i32(dbus, &mut sit, *w);
        append_i32(dbus, &mut sit, *h);
        let ysig = CString::new("y").unwrap();
        let mut bit: DBusMessageIter = std::mem::zeroed();
        (dbus.dbus_message_iter_open_container)(&mut sit, DBUS_TYPE_ARRAY, ysig.as_ptr(), &mut bit);
        // Byte-at-a-time keeps the dlopen surface small
        // (`dbus_message_iter_append_fixed_array` would be the fast path);
        // a 48x48 icon is 9216 appends, invisible next to the bus round trip.
        for b in bytes {
            (dbus.dbus_message_iter_append_basic)(
                &mut bit,
                DBUS_TYPE_BYTE,
                b as *const u8 as *mut c_void,
            );
        }
        (dbus.dbus_message_iter_close_container)(&mut sit, &mut bit);
        (dbus.dbus_message_iter_close_container)(&mut ait, &mut sit);
    }
    (dbus.dbus_message_iter_close_container)(it, &mut ait);
}

/// The `(s a(iiay) s s)` tooltip struct.
unsafe fn append_tooltip(dbus: &DBusLib, it: *mut DBusMessageIter, title: &str) {
    let mut sit: DBusMessageIter = std::mem::zeroed();
    (dbus.dbus_message_iter_open_container)(it, DBUS_TYPE_STRUCT, std::ptr::null(), &mut sit);
    append_str(dbus, &mut sit, DBUS_TYPE_STRING, ""); // icon name
    append_pixmaps(dbus, &mut sit, &[]); // icon pixmaps
    append_str(dbus, &mut sit, DBUS_TYPE_STRING, title);
    append_str(dbus, &mut sit, DBUS_TYPE_STRING, ""); // rich body
    (dbus.dbus_message_iter_close_container)(it, &mut sit);
}

/// Every SNI property, in (name, variant-signature) order — the single source
/// for `Get`, `GetAll` and the introspection XML.
const SNI_PROPS: &[(&str, &str)] = &[
    ("Category", "s"),
    ("Id", "s"),
    ("Title", "s"),
    ("Status", "s"),
    ("WindowId", "i"),
    ("IconName", "s"),
    ("IconPixmap", "a(iiay)"),
    ("OverlayIconName", "s"),
    ("OverlayIconPixmap", "a(iiay)"),
    ("AttentionIconName", "s"),
    ("AttentionIconPixmap", "a(iiay)"),
    ("AttentionMovieName", "s"),
    ("ToolTip", "(sa(iiay)ss)"),
    ("ItemIsMenu", "b"),
    ("Menu", "o"),
];

/// Append one property's VALUE (inside a variant of its signature).
unsafe fn append_prop_value(dbus: &DBusLib, it: &mut DBusMessageIter, st: &SniState, name: &str) {
    match name {
        "Category" => append_str(dbus, it, DBUS_TYPE_STRING, "ApplicationStatus"),
        "Id" => append_str(dbus, it, DBUS_TYPE_STRING, &st.id),
        "Title" => append_str(dbus, it, DBUS_TYPE_STRING, &st.title),
        "Status" => append_str(dbus, it, DBUS_TYPE_STRING, st.status),
        "WindowId" => append_i32(dbus, it, 0),
        "IconName" => append_str(dbus, it, DBUS_TYPE_STRING, &st.icon_name),
        "IconPixmap" => append_pixmaps(dbus, it, &st.pixmaps),
        "OverlayIconName" | "AttentionIconName" | "AttentionMovieName" => {
            append_str(dbus, it, DBUS_TYPE_STRING, "");
        }
        "OverlayIconPixmap" | "AttentionIconPixmap" => append_pixmaps(dbus, it, &[]),
        "ToolTip" => append_tooltip(dbus, it, &st.tooltip),
        // TRUE whenever a menu exists: KDE opens the dbusmenu on LEFT click
        // only for ItemIsMenu items - with false it sent Activate and the
        // menu never showed (dbus-monitor capture of the user's click,
        // 2026-08-29). Menu-less items keep false so Activate still works.
        "ItemIsMenu" => append_bool(dbus, it, st.menu.len() > 1),
        // With a menu: the dbusmenu object the panel renders itself. Without:
        // the root path makes hosts fall back to ContextMenu(), surfaced as a
        // TrayEvent.
        "Menu" => append_str(
            dbus,
            it,
            DBUS_TYPE_OBJECT_PATH,
            if st.menu.len() > 1 { MENU_PATH } else { "/" },
        ),
        _ => append_str(dbus, it, DBUS_TYPE_STRING, ""),
    }
}

// ---- the object-path handler -------------------------------------------------

unsafe extern "C" fn sni_message(
    conn: *mut DBusConnection,
    msg: *mut DBusMessage,
    user_data: *mut c_void,
) -> c_int {
    let st = &*(user_data as *const SniState);
    let dbus = &st.dbus;

    let iface = (dbus.dbus_message_get_interface)(msg);
    let member = (dbus.dbus_message_get_member)(msg);
    if iface.is_null() || member.is_null() {
        return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
    }
    let iface = CStr::from_ptr(iface).to_string_lossy();
    let member = CStr::from_ptr(member).to_string_lossy();

    let reply = match (iface.as_ref(), member.as_ref()) {
        (INTROSPECT_IFACE, "Introspect") => {
            let reply = (dbus.dbus_message_new_method_return)(msg);
            if !reply.is_null() {
                let mut it: DBusMessageIter = std::mem::zeroed();
                (dbus.dbus_message_iter_init_append)(reply, &mut it);
                append_str(dbus, &mut it, DBUS_TYPE_STRING, &introspection_xml());
            }
            reply
        }
        (PROPS_IFACE, "Get") => {
            let (Some(_prop_iface), Some(prop)) = read_two_strings(dbus, msg) else {
                return error_reply(dbus, conn, msg, "org.freedesktop.DBus.Error.InvalidArgs");
            };
            let Some((_, sig)) = SNI_PROPS.iter().find(|(n, _)| *n == prop) else {
                return error_reply(
                    dbus,
                    conn,
                    msg,
                    "org.freedesktop.DBus.Error.UnknownProperty",
                );
            };
            let reply = (dbus.dbus_message_new_method_return)(msg);
            if !reply.is_null() {
                let mut it: DBusMessageIter = std::mem::zeroed();
                (dbus.dbus_message_iter_init_append)(reply, &mut it);
                in_variant(dbus, &mut it, sig, |vit| {
                    append_prop_value(dbus, vit, st, &prop);
                });
            }
            reply
        }
        (PROPS_IFACE, "GetAll") => {
            let reply = (dbus.dbus_message_new_method_return)(msg);
            if !reply.is_null() {
                let mut it: DBusMessageIter = std::mem::zeroed();
                (dbus.dbus_message_iter_init_append)(reply, &mut it);
                let esig = CString::new("{sv}").unwrap();
                let mut ait: DBusMessageIter = std::mem::zeroed();
                (dbus.dbus_message_iter_open_container)(
                    &mut it,
                    DBUS_TYPE_ARRAY,
                    esig.as_ptr(),
                    &mut ait,
                );
                for (name, sig) in SNI_PROPS {
                    let mut eit: DBusMessageIter = std::mem::zeroed();
                    (dbus.dbus_message_iter_open_container)(
                        &mut ait,
                        DBUS_TYPE_DICT_ENTRY,
                        std::ptr::null(),
                        &mut eit,
                    );
                    append_str(dbus, &mut eit, DBUS_TYPE_STRING, name);
                    in_variant(dbus, &mut eit, sig, |vit| {
                        append_prop_value(dbus, vit, st, name);
                    });
                    (dbus.dbus_message_iter_close_container)(&mut ait, &mut eit);
                }
                (dbus.dbus_message_iter_close_container)(&mut it, &mut ait);
            }
            reply
        }
        (SNI_IFACE, "Activate") => {
            queue_tray_event(TrayEvent::simple(TrayEventType::Activate));
            (dbus.dbus_message_new_method_return)(msg)
        }
        (SNI_IFACE, "SecondaryActivate") => {
            queue_tray_event(TrayEvent::simple(TrayEventType::SecondaryActivate));
            (dbus.dbus_message_new_method_return)(msg)
        }
        (SNI_IFACE, "ContextMenu") => {
            queue_tray_event(TrayEvent::simple(TrayEventType::ContextMenu));
            (dbus.dbus_message_new_method_return)(msg)
        }
        (SNI_IFACE, "Scroll") => {
            let mut ev = TrayEvent::simple(TrayEventType::Scroll);
            let mut it: DBusMessageIter = std::mem::zeroed();
            if (dbus.dbus_message_iter_init)(msg, &mut it) != 0
                && (dbus.dbus_message_iter_get_arg_type)(&mut it) == DBUS_TYPE_INT32
            {
                let mut delta: i32 = 0;
                (dbus.dbus_message_iter_get_basic)(&mut it, &mut delta as *mut i32 as *mut c_void);
                ev.scroll_delta = delta;
                if (dbus.dbus_message_iter_next)(&mut it) != 0
                    && (dbus.dbus_message_iter_get_arg_type)(&mut it) == DBUS_TYPE_STRING
                {
                    let mut sp: *const c_char = std::ptr::null();
                    (dbus.dbus_message_iter_get_basic)(
                        &mut it,
                        &mut sp as *mut *const c_char as *mut c_void,
                    );
                    if !sp.is_null()
                        && CStr::from_ptr(sp)
                            .to_string_lossy()
                            .eq_ignore_ascii_case("horizontal")
                    {
                        ev.scroll_axis = TrayScrollAxis::Horizontal;
                    }
                }
            }
            queue_tray_event(ev);
            (dbus.dbus_message_new_method_return)(msg)
        }
        _ => return DBUS_HANDLER_RESULT_NOT_YET_HANDLED,
    };

    if !reply.is_null() {
        (dbus.dbus_connection_send)(conn, reply, std::ptr::null_mut());
        (dbus.dbus_message_unref)(reply);
        (dbus.dbus_connection_flush)(conn);
    }
    DBUS_HANDLER_RESULT_HANDLED
}

unsafe fn error_reply(
    dbus: &DBusLib,
    conn: *mut DBusConnection,
    msg: *mut DBusMessage,
    name: &str,
) -> c_int {
    let n = CString::new(name).unwrap();
    let e = (dbus.dbus_message_new_error)(msg, n.as_ptr(), std::ptr::null());
    if !e.is_null() {
        (dbus.dbus_connection_send)(conn, e, std::ptr::null_mut());
        (dbus.dbus_message_unref)(e);
    }
    DBUS_HANDLER_RESULT_HANDLED
}

/// The first two STRING arguments of a message.
unsafe fn read_two_strings(
    dbus: &DBusLib,
    msg: *mut DBusMessage,
) -> (Option<String>, Option<String>) {
    let mut it: DBusMessageIter = std::mem::zeroed();
    if (dbus.dbus_message_iter_init)(msg, &mut it) == 0 {
        return (None, None);
    }
    let mut out: [Option<String>; 2] = [None, None];
    for slot in &mut out {
        if (dbus.dbus_message_iter_get_arg_type)(&mut it) != DBUS_TYPE_STRING {
            break;
        }
        let mut sp: *const c_char = std::ptr::null();
        (dbus.dbus_message_iter_get_basic)(&mut it, &mut sp as *mut *const c_char as *mut c_void);
        if !sp.is_null() {
            *slot = Some(CStr::from_ptr(sp).to_string_lossy().into_owned());
        }
        (dbus.dbus_message_iter_next)(&mut it);
    }
    (out[0].take(), out[1].take())
}

fn introspection_xml() -> String {
    let mut props = String::new();
    for (name, sig) in SNI_PROPS {
        props.push_str(&format!(
            "    <property name=\"{name}\" type=\"{sig}\" access=\"read\"/>\n"
        ));
    }
    format!(
        "<!DOCTYPE node PUBLIC \"-//freedesktop//DTD D-BUS Object Introspection 1.0//EN\" \
         \"http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd\">\n\
         <node>\n\
         <interface name=\"{SNI_IFACE}\">\n{props}\
         \x20   <method name=\"Activate\"><arg name=\"x\" type=\"i\" direction=\"in\"/><arg name=\"y\" type=\"i\" direction=\"in\"/></method>\n\
         \x20   <method name=\"SecondaryActivate\"><arg name=\"x\" type=\"i\" direction=\"in\"/><arg name=\"y\" type=\"i\" direction=\"in\"/></method>\n\
         \x20   <method name=\"ContextMenu\"><arg name=\"x\" type=\"i\" direction=\"in\"/><arg name=\"y\" type=\"i\" direction=\"in\"/></method>\n\
         \x20   <method name=\"Scroll\"><arg name=\"delta\" type=\"i\" direction=\"in\"/><arg name=\"orientation\" type=\"s\" direction=\"in\"/></method>\n\
         \x20   <signal name=\"NewIcon\"/>\n\
         \x20   <signal name=\"NewTitle\"/>\n\
         \x20   <signal name=\"NewToolTip\"/>\n\
         \x20   <signal name=\"NewStatus\"><arg name=\"status\" type=\"s\"/></signal>\n\
         </interface>\n\
         <interface name=\"{PROPS_IFACE}\">\n\
         \x20   <method name=\"Get\"><arg type=\"s\" direction=\"in\"/><arg type=\"s\" direction=\"in\"/><arg type=\"v\" direction=\"out\"/></method>\n\
         \x20   <method name=\"GetAll\"><arg type=\"s\" direction=\"in\"/><arg type=\"a{{sv}}\" direction=\"out\"/></method>\n\
         </interface>\n\
         <interface name=\"{INTROSPECT_IFACE}\">\n\
         \x20   <method name=\"Introspect\"><arg type=\"s\" direction=\"out\"/></method>\n\
         </interface>\n\
         </node>\n"
    )
}

// ---- the dbusmenu object (/MenuBar) ------------------------------------------
//
// com.canonical.dbusmenu, the protocol every SNI host (KDE, most Wayland
// panels, Ayatana) uses to render a tray MENU: the panel calls GetLayout,
// draws the tree itself, and reports clicks back through Event("clicked").
// Serving it is what turns the silent heart-icon into a working menu - the
// item's `Menu` property points here whenever the app supplied one.

/// The a{sv} property map of one layout node.
unsafe fn append_menu_props(dbus: &DBusLib, it: *mut DBusMessageIter, node: &MenuNode) {
    let esig = CString::new("{sv}").unwrap();
    let mut ait: DBusMessageIter = std::mem::zeroed();
    (dbus.dbus_message_iter_open_container)(it, DBUS_TYPE_ARRAY, esig.as_ptr(), &mut ait);
    let mut put = |k: &str, sig: &str, f: &dyn Fn(&mut DBusMessageIter)| {
        let mut eit: DBusMessageIter = std::mem::zeroed();
        (dbus.dbus_message_iter_open_container)(
            &mut ait,
            DBUS_TYPE_DICT_ENTRY,
            std::ptr::null(),
            &mut eit,
        );
        append_str(dbus, &mut eit, DBUS_TYPE_STRING, k);
        in_variant(dbus, &mut eit, sig, |vit| f(vit));
        (dbus.dbus_message_iter_close_container)(&mut ait, &mut eit);
    };
    if node.separator {
        put("type", "s", &|vit| {
            append_str(dbus, vit, DBUS_TYPE_STRING, "separator");
        });
    } else {
        put("label", "s", &|vit| {
            append_str(dbus, vit, DBUS_TYPE_STRING, &node.label);
        });
        put("enabled", "b", &|vit| append_bool(dbus, vit, node.enabled));
        if !node.children.is_empty() {
            put("children-display", "s", &|vit| {
                append_str(dbus, vit, DBUS_TYPE_STRING, "submenu");
            });
        }
    }
    put("visible", "b", &|vit| append_bool(dbus, vit, true));
    (dbus.dbus_message_iter_close_container)(it, &mut ait);
}

/// One `(ia{sv}av)` layout node, recursively (depth < 0 = unlimited, the
/// value KDE sends).
unsafe fn append_layout_node(
    dbus: &DBusLib,
    it: *mut DBusMessageIter,
    menu: &[MenuNode],
    id: i32,
    depth: i32,
) {
    let Some(node) = menu.get(id as usize) else {
        return;
    };
    let mut sit: DBusMessageIter = std::mem::zeroed();
    (dbus.dbus_message_iter_open_container)(it, DBUS_TYPE_STRUCT, std::ptr::null(), &mut sit);
    append_i32(dbus, &mut sit, id);
    append_menu_props(dbus, &mut sit, node);
    let vsig = CString::new("v").unwrap();
    let mut cit: DBusMessageIter = std::mem::zeroed();
    (dbus.dbus_message_iter_open_container)(&mut sit, DBUS_TYPE_ARRAY, vsig.as_ptr(), &mut cit);
    if depth != 0 {
        let child_depth = if depth < 0 { -1 } else { depth - 1 };
        for &child in &node.children {
            let ssig = CString::new("(ia{sv}av)").unwrap();
            let mut vit: DBusMessageIter = std::mem::zeroed();
            (dbus.dbus_message_iter_open_container)(
                &mut cit,
                DBUS_TYPE_VARIANT,
                ssig.as_ptr(),
                &mut vit,
            );
            append_layout_node(dbus, &mut vit, menu, child, child_depth);
            (dbus.dbus_message_iter_close_container)(&mut cit, &mut vit);
        }
    }
    (dbus.dbus_message_iter_close_container)(&mut sit, &mut cit);
    (dbus.dbus_message_iter_close_container)(it, &mut sit);
}

fn menu_introspection_xml() -> String {
    r#"<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd">
<node>
  <interface name="com.canonical.dbusmenu">
    <property name="Version" type="u" access="read"/>
    <property name="Status" type="s" access="read"/>
    <property name="TextDirection" type="s" access="read"/>
    <method name="GetLayout">
      <arg type="i" name="parentId" direction="in"/>
      <arg type="i" name="recursionDepth" direction="in"/>
      <arg type="as" name="propertyNames" direction="in"/>
      <arg type="u" name="revision" direction="out"/>
      <arg type="(ia{sv}av)" name="layout" direction="out"/>
    </method>
    <method name="GetGroupProperties">
      <arg type="ai" name="ids" direction="in"/>
      <arg type="as" name="propertyNames" direction="in"/>
      <arg type="a(ia{sv})" name="properties" direction="out"/>
    </method>
    <method name="Event">
      <arg type="i" name="id" direction="in"/>
      <arg type="s" name="eventId" direction="in"/>
      <arg type="v" name="data" direction="in"/>
      <arg type="u" name="timestamp" direction="in"/>
    </method>
    <method name="EventGroup">
      <arg type="a(isvu)" name="events" direction="in"/>
      <arg type="ai" name="idErrors" direction="out"/>
    </method>
    <method name="AboutToShow">
      <arg type="i" name="id" direction="in"/>
      <arg type="b" name="needUpdate" direction="out"/>
    </method>
    <method name="AboutToShowGroup">
      <arg type="ai" name="ids" direction="in"/>
      <arg type="ai" name="updatesNeeded" direction="out"/>
      <arg type="ai" name="idErrors" direction="out"/>
    </method>
    <signal name="LayoutUpdated">
      <arg type="u" name="revision"/>
      <arg type="i" name="parent"/>
    </signal>
    <signal name="ItemsPropertiesUpdated">
      <arg type="a(ia{sv})" name="updatedProps"/>
      <arg type="a(ias)" name="removedProps"/>
    </signal>
  </interface>
  <interface name="org.freedesktop.DBus.Properties">
    <method name="Get">
      <arg type="s" name="interface_name" direction="in"/>
      <arg type="s" name="property_name" direction="in"/>
      <arg type="v" name="value" direction="out"/>
    </method>
    <method name="GetAll">
      <arg type="s" name="interface_name" direction="in"/>
      <arg type="a{sv}" name="properties" direction="out"/>
    </method>
  </interface>
</node>"#
        .to_owned()
}

/// The first `i32` argument of a message (dbusmenu ids), if any.
unsafe fn read_first_i32(dbus: &DBusLib, msg: *mut DBusMessage) -> Option<i32> {
    let mut it: DBusMessageIter = std::mem::zeroed();
    if (dbus.dbus_message_iter_init)(msg, &mut it) == 0 {
        return None;
    }
    if (dbus.dbus_message_iter_get_arg_type)(&mut it) != DBUS_TYPE_INT32 {
        return None;
    }
    let mut v: i32 = 0;
    (dbus.dbus_message_iter_get_basic)(&mut it, &mut v as *mut i32 as *mut c_void);
    Some(v)
}

unsafe extern "C" fn menu_message(
    conn: *mut DBusConnection,
    msg: *mut DBusMessage,
    user_data: *mut c_void,
) -> c_int {
    let st = &*(user_data as *const SniState);
    let dbus = &st.dbus;

    let iface = (dbus.dbus_message_get_interface)(msg);
    let member = (dbus.dbus_message_get_member)(msg);
    if iface.is_null() || member.is_null() {
        return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
    }
    let iface = CStr::from_ptr(iface).to_string_lossy();
    let member = CStr::from_ptr(member).to_string_lossy();

    let reply = match (iface.as_ref(), member.as_ref()) {
        (INTROSPECT_IFACE, "Introspect") => {
            let reply = (dbus.dbus_message_new_method_return)(msg);
            if !reply.is_null() {
                let mut it: DBusMessageIter = std::mem::zeroed();
                (dbus.dbus_message_iter_init_append)(reply, &mut it);
                append_str(dbus, &mut it, DBUS_TYPE_STRING, &menu_introspection_xml());
            }
            reply
        }
        (PROPS_IFACE, "Get") => {
            let (_, Some(prop)) = read_two_strings(dbus, msg) else {
                return error_reply(dbus, conn, msg, "org.freedesktop.DBus.Error.InvalidArgs");
            };
            let reply = (dbus.dbus_message_new_method_return)(msg);
            if !reply.is_null() {
                let mut it: DBusMessageIter = std::mem::zeroed();
                (dbus.dbus_message_iter_init_append)(reply, &mut it);
                match prop.as_str() {
                    "Version" => in_variant(dbus, &mut it, "u", |vit| append_u32(dbus, vit, 3)),
                    "TextDirection" => in_variant(dbus, &mut it, "s", |vit| {
                        append_str(dbus, vit, DBUS_TYPE_STRING, "ltr");
                    }),
                    _ => in_variant(dbus, &mut it, "s", |vit| {
                        append_str(dbus, vit, DBUS_TYPE_STRING, "normal");
                    }),
                }
            }
            reply
        }
        (PROPS_IFACE, "GetAll") => {
            let reply = (dbus.dbus_message_new_method_return)(msg);
            if !reply.is_null() {
                let mut it: DBusMessageIter = std::mem::zeroed();
                (dbus.dbus_message_iter_init_append)(reply, &mut it);
                let esig = CString::new("{sv}").unwrap();
                let mut ait: DBusMessageIter = std::mem::zeroed();
                (dbus.dbus_message_iter_open_container)(
                    &mut it,
                    DBUS_TYPE_ARRAY,
                    esig.as_ptr(),
                    &mut ait,
                );
                let mut put = |k: &str, sig: &str, f: &dyn Fn(&mut DBusMessageIter)| {
                    let mut eit: DBusMessageIter = std::mem::zeroed();
                    (dbus.dbus_message_iter_open_container)(
                        &mut ait,
                        DBUS_TYPE_DICT_ENTRY,
                        std::ptr::null(),
                        &mut eit,
                    );
                    append_str(dbus, &mut eit, DBUS_TYPE_STRING, k);
                    in_variant(dbus, &mut eit, sig, |vit| f(vit));
                    (dbus.dbus_message_iter_close_container)(&mut ait, &mut eit);
                };
                put("Version", "u", &|vit| append_u32(dbus, vit, 3));
                put("Status", "s", &|vit| {
                    append_str(dbus, vit, DBUS_TYPE_STRING, "normal");
                });
                put("TextDirection", "s", &|vit| {
                    append_str(dbus, vit, DBUS_TYPE_STRING, "ltr");
                });
                (dbus.dbus_message_iter_close_container)(&mut it, &mut ait);
            }
            reply
        }
        (MENU_IFACE, "GetLayout") => {
            let parent = read_first_i32(dbus, msg).unwrap_or(0);
            // recursionDepth is the SECOND i32; -1 (KDE's value) = unlimited.
            let depth = {
                let mut it: DBusMessageIter = std::mem::zeroed();
                let mut depth = -1;
                if (dbus.dbus_message_iter_init)(msg, &mut it) != 0
                    && (dbus.dbus_message_iter_next)(&mut it) != 0
                    && (dbus.dbus_message_iter_get_arg_type)(&mut it) == DBUS_TYPE_INT32
                {
                    (dbus.dbus_message_iter_get_basic)(
                        &mut it,
                        &mut depth as *mut i32 as *mut c_void,
                    );
                }
                depth
            };
            let reply = (dbus.dbus_message_new_method_return)(msg);
            if !reply.is_null() {
                let mut it: DBusMessageIter = std::mem::zeroed();
                (dbus.dbus_message_iter_init_append)(reply, &mut it);
                append_u32(dbus, &mut it, st.menu_revision);
                append_layout_node(dbus, &mut it, &st.menu, parent, depth);
            }
            reply
        }
        (MENU_IFACE, "GetGroupProperties") => {
            // ids: ai. Reply a(ia{sv}) for every KNOWN id requested (all of
            // them when the array is absent/empty - KDE asks for all).
            let mut ids: Vec<i32> = Vec::new();
            let mut it: DBusMessageIter = std::mem::zeroed();
            if (dbus.dbus_message_iter_init)(msg, &mut it) != 0
                && (dbus.dbus_message_iter_get_arg_type)(&mut it) == DBUS_TYPE_ARRAY
            {
                let mut ait: DBusMessageIter = std::mem::zeroed();
                (dbus.dbus_message_iter_recurse)(&mut it, &mut ait);
                while (dbus.dbus_message_iter_get_arg_type)(&mut ait) == DBUS_TYPE_INT32 {
                    let mut v: i32 = 0;
                    (dbus.dbus_message_iter_get_basic)(&mut ait, &mut v as *mut i32 as *mut c_void);
                    ids.push(v);
                    if (dbus.dbus_message_iter_next)(&mut ait) == 0 {
                        break;
                    }
                }
            }
            if ids.is_empty() {
                ids = (0..st.menu.len() as i32).collect();
            }
            let reply = (dbus.dbus_message_new_method_return)(msg);
            if !reply.is_null() {
                let mut it: DBusMessageIter = std::mem::zeroed();
                (dbus.dbus_message_iter_init_append)(reply, &mut it);
                let ssig = CString::new("(ia{sv})").unwrap();
                let mut ait: DBusMessageIter = std::mem::zeroed();
                (dbus.dbus_message_iter_open_container)(
                    &mut it,
                    DBUS_TYPE_ARRAY,
                    ssig.as_ptr(),
                    &mut ait,
                );
                for id in ids {
                    if let Some(node) = st.menu.get(id as usize) {
                        let mut sit: DBusMessageIter = std::mem::zeroed();
                        (dbus.dbus_message_iter_open_container)(
                            &mut ait,
                            DBUS_TYPE_STRUCT,
                            std::ptr::null(),
                            &mut sit,
                        );
                        append_i32(dbus, &mut sit, id);
                        append_menu_props(dbus, &mut sit, node);
                        (dbus.dbus_message_iter_close_container)(&mut ait, &mut sit);
                    }
                }
                (dbus.dbus_message_iter_close_container)(&mut it, &mut ait);
            }
            reply
        }
        (MENU_IFACE, "Event") => {
            // (id i32, eventId s, data v, timestamp u) - only "clicked" acts.
            let mut it: DBusMessageIter = std::mem::zeroed();
            if (dbus.dbus_message_iter_init)(msg, &mut it) != 0
                && (dbus.dbus_message_iter_get_arg_type)(&mut it) == DBUS_TYPE_INT32
            {
                let mut id: i32 = 0;
                (dbus.dbus_message_iter_get_basic)(&mut it, &mut id as *mut i32 as *mut c_void);
                if (dbus.dbus_message_iter_next)(&mut it) != 0
                    && (dbus.dbus_message_iter_get_arg_type)(&mut it) == DBUS_TYPE_STRING
                {
                    let mut sp: *const c_char = std::ptr::null();
                    (dbus.dbus_message_iter_get_basic)(
                        &mut it,
                        &mut sp as *mut *const c_char as *mut c_void,
                    );
                    if !sp.is_null() && CStr::from_ptr(sp).to_string_lossy() == "clicked" {
                        if let Ok(mut q) = st.clicked.lock() {
                            q.push(id);
                        }
                    }
                }
            }
            (dbus.dbus_message_new_method_return)(msg)
        }
        (MENU_IFACE, "AboutToShow") => {
            let reply = (dbus.dbus_message_new_method_return)(msg);
            if !reply.is_null() {
                let mut it: DBusMessageIter = std::mem::zeroed();
                (dbus.dbus_message_iter_init_append)(reply, &mut it);
                append_bool(dbus, &mut it, false);
            }
            reply
        }
        (MENU_IFACE, "AboutToShowGroup") | (MENU_IFACE, "EventGroup") => {
            // Group forms: acknowledge with empty arrays - KDE's importer
            // sends the singular calls for actual interaction.
            let reply = (dbus.dbus_message_new_method_return)(msg);
            if !reply.is_null() {
                let mut it: DBusMessageIter = std::mem::zeroed();
                (dbus.dbus_message_iter_init_append)(reply, &mut it);
                let isig = CString::new("i").unwrap();
                let n_arrays = if member.as_ref() == "AboutToShowGroup" {
                    2
                } else {
                    1
                };
                for _ in 0..n_arrays {
                    let mut ait: DBusMessageIter = std::mem::zeroed();
                    (dbus.dbus_message_iter_open_container)(
                        &mut it,
                        DBUS_TYPE_ARRAY,
                        isig.as_ptr(),
                        &mut ait,
                    );
                    (dbus.dbus_message_iter_close_container)(&mut it, &mut ait);
                }
            }
            reply
        }
        _ => return DBUS_HANDLER_RESULT_NOT_YET_HANDLED,
    };

    if !reply.is_null() {
        (dbus.dbus_connection_send)(conn, reply, std::ptr::null_mut());
        (dbus.dbus_message_unref)(reply);
        (dbus.dbus_connection_flush)(conn);
    }
    DBUS_HANDLER_RESULT_HANDLED
}

static MENU_VTABLE: DBusObjectPathVTable = DBusObjectPathVTable {
    unregister_function: None,
    message_function: Some(menu_message),
    dbus_internal_padding1: None,
    dbus_internal_padding2: None,
    dbus_internal_padding3: None,
    dbus_internal_padding4: None,
};

static SNI_VTABLE: DBusObjectPathVTable = DBusObjectPathVTable {
    unregister_function: None,
    message_function: Some(sni_message),
    dbus_internal_padding1: None,
    dbus_internal_padding2: None,
    dbus_internal_padding3: None,
    dbus_internal_padding4: None,
};

// ---- state building ----------------------------------------------------------

/// RGBA8 (non-premultiplied) to the ARGB32 big-endian bytes SNI wants.
fn rgba_to_argb_be(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        out.extend_from_slice(&[px[3], px[0], px[1], px[2]]);
    }
    out
}

fn build_pixmaps(
    data: &TrayIconData,
    provider: &azul_core::icon::SharedIconProvider,
    font_manager: &azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>,
) -> Vec<(i32, i32, Vec<u8>)> {
    let mut out = Vec::new();
    match &data.icon {
        TrayIconSource::Named(spec) => {
            // Rasterize through the icon registry at the panel's common sizes;
            // never pass the spec as a theme IconName the panel may not have.
            for size in [22u32, 48u32] {
                if let Some(icon) = render_named_icon(spec.as_str(), size, provider, font_manager) {
                    out.push((
                        icon.width as i32,
                        icon.height as i32,
                        rgba_to_argb_be(&icon.rgba),
                    ));
                }
            }
        }
        TrayIconSource::Rgba(_) => {
            for size in [22u32, 48u32] {
                if let azul_core::tray::OptionTrayIconImage::Some(img) = data.best_icon(size) {
                    let (w, h) = (img.width as i32, img.height as i32);
                    if !out.iter().any(|(ow, oh, _)| *ow == w && *oh == h) {
                        out.push((w, h, img.to_argb32_be().as_ref().to_vec()));
                    }
                }
            }
        }
        TrayIconSource::None => {}
    }
    out
}

fn build_state(
    dbus: Arc<DBusLib>,
    data: &TrayIconData,
    provider: &azul_core::icon::SharedIconProvider,
    font_manager: &azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>,
) -> SniState {
    let pixmaps = build_pixmaps(data, provider, font_manager);
    SniState {
        dbus,
        id: data.id.as_str().to_owned(),
        title: data.title.as_str().to_owned(),
        tooltip: data
            .tooltip
            .as_ref()
            .map(|t| t.as_str().to_owned())
            .unwrap_or_else(|| data.title.as_str().to_owned()),
        status: match data.status {
            TrayStatus::Passive => "Passive",
            TrayStatus::Active => "Active",
            TrayStatus::NeedsAttention => "NeedsAttention",
        },
        // An item with neither pixmap nor name is invisible, which looks
        // exactly like a working icon nobody can find. Fall back to a themed
        // name every icon theme carries.
        icon_name: if pixmaps.is_empty() {
            "application-x-executable".to_owned()
        } else {
            String::new()
        },
        pixmaps,
        menu: flatten_menu(data.menu.as_ref()),
        menu_revision: 1,
        clicked: std::sync::Mutex::new(Vec::new()),
    }
}

impl PlatformTray {
    pub(super) fn new(
        data: &TrayIconData,
        provider: &azul_core::icon::SharedIconProvider,
        font_manager: &azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>,
    ) -> Result<Self, TrayError> {
        let dbus = get_shared_dbus_lib().ok_or(TrayError::Unavailable)?;
        if !is_available() {
            return Err(TrayError::Unavailable);
        }
        unsafe {
            let mut err = fresh_error();
            (dbus.dbus_error_init)(&mut err);
            let conn = (dbus.dbus_bus_get)(DBUS_BUS_SESSION, &mut err);
            if conn.is_null() {
                (dbus.dbus_error_free)(&mut err);
                return Err(TrayError::Platform("no session bus".into()));
            }

            // org.kde.*, not org.freedesktop.* — see the module docs.
            let bus_name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
            let bus_name_c = CString::new(bus_name.clone()).unwrap();
            let got = (dbus.dbus_bus_request_name)(
                conn,
                bus_name_c.as_ptr(),
                crate::desktop::shell2::linux::dbus::DBUS_NAME_FLAG_DO_NOT_QUEUE,
                &mut err,
            );
            if (dbus.dbus_error_is_set)(&err) != 0 || got != 1 {
                (dbus.dbus_error_free)(&mut err);
                return Err(TrayError::Platform(format!(
                    "could not own {bus_name} (result {got})"
                )));
            }

            let state = Box::new(build_state(dbus.clone(), data, provider, font_manager));
            let path_c = CString::new(SNI_PATH).unwrap();
            let ok = (dbus.dbus_connection_register_object_path)(
                conn,
                path_c.as_ptr(),
                &SNI_VTABLE,
                &*state as *const SniState as *mut c_void,
            );
            if ok == 0 {
                return Err(TrayError::Platform(
                    "dbus_connection_register_object_path failed".into(),
                ));
            }

            // The dbusmenu object (same state box). Registered even for a
            // menu-less item: the Menu property then points at "/" and the
            // path simply never gets called.
            let mpath_c = CString::new(MENU_PATH).unwrap();
            let ok = (dbus.dbus_connection_register_object_path)(
                conn,
                mpath_c.as_ptr(),
                &MENU_VTABLE,
                &*state as *const SniState as *mut c_void,
            );
            if ok == 0 {
                return Err(TrayError::Platform(
                    "dbus_connection_register_object_path (/MenuBar) failed".into(),
                ));
            }

            // RegisterStatusNotifierItem(our bus name) at the watcher.
            let dest = CString::new(WATCHER_NAME).unwrap();
            let wpath = CString::new(WATCHER_PATH).unwrap();
            let wiface = CString::new(WATCHER_NAME).unwrap();
            let member = CString::new("RegisterStatusNotifierItem").unwrap();
            let msg = (dbus.dbus_message_new_method_call)(
                dest.as_ptr(),
                wpath.as_ptr(),
                wiface.as_ptr(),
                member.as_ptr(),
            );
            if msg.is_null() {
                return Err(TrayError::Platform("message alloc failed".into()));
            }
            let mut it: DBusMessageIter = std::mem::zeroed();
            (dbus.dbus_message_iter_init_append)(msg, &mut it);
            append_str(&dbus, &mut it, DBUS_TYPE_STRING, &bus_name);
            let reply = (dbus.dbus_connection_send_with_reply_and_block)(conn, msg, 2000, &mut err);
            (dbus.dbus_message_unref)(msg);
            if reply.is_null() {
                let m = if !err.message.is_null() {
                    CStr::from_ptr(err.message).to_string_lossy().into_owned()
                } else {
                    "watcher did not answer RegisterStatusNotifierItem".into()
                };
                (dbus.dbus_error_free)(&mut err);
                return Err(TrayError::Platform(m));
            }
            (dbus.dbus_message_unref)(reply);
            (dbus.dbus_connection_flush)(conn);

            Ok(Self { dbus, conn, state })
        }
    }

    pub(super) fn update(
        &mut self,
        _old: &TrayIconData,
        new: &TrayIconData,
        provider: &azul_core::icon::SharedIconProvider,
        font_manager: &azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>,
    ) -> Result<(), TrayError> {
        // Mutate through the SAME box the vtable points at. The layout
        // revision must move FORWARD across the rebuild or hosts ignore the
        // LayoutUpdated below as stale.
        let next_revision = self.state.menu_revision.wrapping_add(1);
        *self.state = build_state(self.dbus.clone(), new, provider, font_manager);
        self.state.menu_revision = next_revision;
        // Hosts only re-read a property after its New* signal.
        unsafe {
            for signal in ["NewIcon", "NewTitle", "NewToolTip"] {
                self.emit_signal(signal, None);
            }
            self.emit_signal("NewStatus", Some(self.state.status));
            // dbusmenu: tell the panel the tree changed (revision, parent 0).
            let path = CString::new(MENU_PATH).unwrap();
            let iface = CString::new(MENU_IFACE).unwrap();
            let member = CString::new("LayoutUpdated").unwrap();
            let msg =
                (self.dbus.dbus_message_new_signal)(path.as_ptr(), iface.as_ptr(), member.as_ptr());
            if !msg.is_null() {
                let mut it: DBusMessageIter = std::mem::zeroed();
                (self.dbus.dbus_message_iter_init_append)(msg, &mut it);
                append_u32(&self.dbus, &mut it, next_revision);
                append_i32(&self.dbus, &mut it, 0);
                (self.dbus.dbus_connection_send)(self.conn, msg, std::ptr::null_mut());
                (self.dbus.dbus_message_unref)(msg);
                (self.dbus.dbus_connection_flush)(self.conn);
            }
        }
        Ok(())
    }

    unsafe fn emit_signal(&self, name: &str, arg: Option<&str>) {
        let path = CString::new(SNI_PATH).unwrap();
        let iface = CString::new(SNI_IFACE).unwrap();
        let member = CString::new(name).unwrap();
        let msg =
            (self.dbus.dbus_message_new_signal)(path.as_ptr(), iface.as_ptr(), member.as_ptr());
        if msg.is_null() {
            return;
        }
        if let Some(arg) = arg {
            let mut it: DBusMessageIter = std::mem::zeroed();
            (self.dbus.dbus_message_iter_init_append)(msg, &mut it);
            append_str(&self.dbus, &mut it, DBUS_TYPE_STRING, arg);
        }
        (self.dbus.dbus_connection_send)(self.conn, msg, std::ptr::null_mut());
        (self.dbus.dbus_message_unref)(msg);
        (self.dbus.dbus_connection_flush)(self.conn);
    }

    /// Dispatch incoming D-Bus traffic (property reads, Activate calls,
    /// dbusmenu GetLayout/Event), then hand back the menu callbacks the
    /// host's clicks selected - the run loop invokes them against a window
    /// exactly like macOS's `pump_tray_into_windows`.
    pub(super) fn pump(&mut self) -> Vec<azul_core::menu::CoreMenuCallback> {
        unsafe {
            (self.dbus.dbus_connection_read_write_dispatch)(self.conn, 0);
        }
        let ids: Vec<i32> = match self.state.clicked.lock() {
            Ok(mut q) => q.drain(..).collect(),
            Err(_) => Vec::new(),
        };
        ids.into_iter()
            .filter_map(|id| {
                self.state
                    .menu
                    .get(id as usize)
                    .and_then(|n| n.callback.as_ref().cloned())
            })
            .collect()
    }
}
