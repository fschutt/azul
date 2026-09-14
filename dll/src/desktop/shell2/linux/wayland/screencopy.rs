//! Wayland window capture via `ext-image-copy-capture-v1`.
//!
//! WHY THIS EXISTS. On X11 a client can read its own window straight off the
//! screen (`XGetImage` on the root at the frame's rectangle, see
//! `desktop::native_screenshot`). Wayland has no such call by design: a client
//! cannot read the compositor's output, and azul requests SERVER-SIDE
//! decorations wherever `xdg-decoration` exists, so the titlebar is not even
//! part of the surface it paints. `take_native_screenshot` therefore refused
//! on Wayland entirely.
//!
//! The way in is `ext-image-copy-capture-v1` (staging, the cross-compositor
//! successor to wlroots' `wlr-screencopy-unstable-v1`), which captures a
//! source AS THE COMPOSITOR SEES IT — decorations included. The source comes
//! from `ext-image-capture-source-v1`, and for a window rather than a whole
//! output that means a toplevel handle out of `ext-foreign-toplevel-list-v1`.
//!
//! WHAT IS NOT VERIFIED. This was written against the published protocol
//! definitions without a Wayland session to run it in — the machine it was
//! developed on is X11. The opcode order below is the protocols' declaration
//! order (opcodes are assigned by declaration order) taken from the upstream
//! XML, and is the first thing to re-check if a compositor rejects a request.
//! Every step logs, and every failure is a graceful `Err`, never a panic: on a
//! compositor that does not advertise these globals the caller simply gets the
//! same "not supported" it got before.
//!
//! SELF-IDENTIFICATION is the protocol's weak point. There is no "capture my
//! own surface" source — the only window source is a foreign-toplevel handle,
//! and the list hands out every toplevel on the desktop with no marker for
//! which one is ours. We match on title, and on app_id when one is set. Two
//! windows with the same title is an ambiguity the protocol gives us no way to
//! resolve, so that case is reported rather than guessed at.

#![cfg(target_os = "linux")]

// `c_char`, never `i8`: it is `i8` on x86_64 but `u8` on aarch64, riscv64,
// powerpc64 and armv7 Linux, so spelling C strings `*const i8` compiled here
// and broke every one of those cross builds at `CStr::from_ptr`.
use core::ffi::{c_char, c_void};

use azul_css::AzString;

use super::{defines, dlopen::Wayland};

// ── Interface tables ────────────────────────────────────────────────────────
//
// Hand-built for the same reason the xdg_shell and xdg-decoration tables in
// `defines.rs` are: libwayland exports interfaces for the core protocol only,
// and these are protocol EXTENSIONS. The shape follows `defines.rs` exactly.
//
// `types` matters in one place and is inert everywhere else. For a REQUEST the
// target interface is passed to `wl_proxy_marshal_constructor` explicitly, so a
// null entry is fine. For an EVENT carrying a `new_id` libwayland has to
// construct the proxy itself and reads the interface out of `types` — a null
// there is a crash, not a warning. Exactly one event here is in that position:
// `ext_foreign_toplevel_list_v1.toplevel`.

macro_rules! leak_null_types {
    ($n:expr) => {{
        let v: &'static [*const defines::wl_interface] =
            Box::leak(vec![core::ptr::null(); $n].into_boxed_slice());
        v
    }};
}

/// `ext_image_capture_source_v1` — an opaque handle. destroy() = opcode 0.
pub fn ext_image_capture_source_v1_interface() -> &'static defines::wl_interface {
    use std::sync::OnceLock;
    static I: OnceLock<defines::SyncInterface> = OnceLock::new();
    I.get_or_init(|| {
        defines::SyncInterface({
            let nt = leak_null_types!(4);
            let requests: &'static [defines::wl_message] =
                Box::leak(Box::new([defines::wl_message {
                    name: b"destroy\0".as_ptr() as _,
                    signature: b"\0".as_ptr() as _,
                    types: nt.as_ptr(),
                }]));
            Box::leak(Box::new(defines::wl_interface {
                name: b"ext_image_capture_source_v1\0".as_ptr() as _,
                version: 1,
                method_count: 1,
                methods: requests.as_ptr(),
                event_count: 0,
                events: core::ptr::null(),
            }))
        })
    })
    .0
}

/// `ext_foreign_toplevel_image_capture_source_manager_v1`.
/// Requests: create_source(new_id<source>, object<handle>) = 0, destroy = 1.
pub fn ext_foreign_toplevel_image_capture_source_manager_v1_interface()
-> &'static defines::wl_interface {
    use std::sync::OnceLock;
    static I: OnceLock<defines::SyncInterface> = OnceLock::new();
    I.get_or_init(|| {
        defines::SyncInterface({
            let nt = leak_null_types!(4);
            let requests: &'static [defines::wl_message] = Box::leak(Box::new([
                defines::wl_message {
                    name: b"create_source\0".as_ptr() as _,
                    signature: b"no\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
                defines::wl_message {
                    name: b"destroy\0".as_ptr() as _,
                    signature: b"\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
            ]));
            Box::leak(Box::new(defines::wl_interface {
                name: b"ext_foreign_toplevel_image_capture_source_manager_v1\0".as_ptr() as _,
                version: 1,
                method_count: 2,
                methods: requests.as_ptr(),
                event_count: 0,
                events: core::ptr::null(),
            }))
        })
    })
    .0
}

/// `ext_image_copy_capture_manager_v1`.
/// Requests: create_session(new_id<session>, object<source>, uint options) = 0,
/// create_pointer_cursor_session(...) = 1, destroy = 2.
///
/// `create_pointer_cursor_session` is declared even though it is never called:
/// opcodes are positional, and leaving it out would silently renumber
/// `destroy`.
pub fn ext_image_copy_capture_manager_v1_interface() -> &'static defines::wl_interface {
    use std::sync::OnceLock;
    static I: OnceLock<defines::SyncInterface> = OnceLock::new();
    I.get_or_init(|| {
        defines::SyncInterface({
            let nt = leak_null_types!(4);
            let requests: &'static [defines::wl_message] = Box::leak(Box::new([
                defines::wl_message {
                    name: b"create_session\0".as_ptr() as _,
                    signature: b"nou\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
                defines::wl_message {
                    name: b"create_pointer_cursor_session\0".as_ptr() as _,
                    signature: b"noo\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
                defines::wl_message {
                    name: b"destroy\0".as_ptr() as _,
                    signature: b"\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
            ]));
            Box::leak(Box::new(defines::wl_interface {
                name: b"ext_image_copy_capture_manager_v1\0".as_ptr() as _,
                version: 1,
                method_count: 3,
                methods: requests.as_ptr(),
                event_count: 0,
                events: core::ptr::null(),
            }))
        })
    })
    .0
}

/// `ext_image_copy_capture_session_v1`.
/// Requests: create_frame(new_id<frame>) = 0, destroy = 1.
/// Events: buffer_size(uu) = 0, shm_format(u) = 1, dmabuf_device(a) = 2,
/// dmabuf_format(ua) = 3, done() = 4, stopped() = 5.
pub fn ext_image_copy_capture_session_v1_interface() -> &'static defines::wl_interface {
    use std::sync::OnceLock;
    static I: OnceLock<defines::SyncInterface> = OnceLock::new();
    I.get_or_init(|| {
        defines::SyncInterface({
            let nt = leak_null_types!(4);
            let requests: &'static [defines::wl_message] = Box::leak(Box::new([
                defines::wl_message {
                    name: b"create_frame\0".as_ptr() as _,
                    signature: b"n\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
                defines::wl_message {
                    name: b"destroy\0".as_ptr() as _,
                    signature: b"\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
            ]));
            let events: &'static [defines::wl_message] = Box::leak(Box::new([
                defines::wl_message {
                    name: b"buffer_size\0".as_ptr() as _,
                    signature: b"uu\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
                defines::wl_message {
                    name: b"shm_format\0".as_ptr() as _,
                    signature: b"u\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
                defines::wl_message {
                    name: b"dmabuf_device\0".as_ptr() as _,
                    signature: b"a\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
                defines::wl_message {
                    name: b"dmabuf_format\0".as_ptr() as _,
                    signature: b"ua\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
                defines::wl_message {
                    name: b"done\0".as_ptr() as _,
                    signature: b"\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
                defines::wl_message {
                    name: b"stopped\0".as_ptr() as _,
                    signature: b"\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
            ]));
            Box::leak(Box::new(defines::wl_interface {
                name: b"ext_image_copy_capture_session_v1\0".as_ptr() as _,
                version: 1,
                method_count: 2,
                methods: requests.as_ptr(),
                event_count: 6,
                events: events.as_ptr(),
            }))
        })
    })
    .0
}

/// `ext_image_copy_capture_frame_v1`.
/// Requests: destroy = 0, attach_buffer(object<wl_buffer>) = 1,
/// damage_buffer(iiii) = 2, capture() = 3.
/// Events: transform(u) = 0, damage(iiii) = 1,
/// presentation_time(uuu) = 2, ready() = 3, failed(u) = 4.
///
/// Note `destroy` is opcode 0 here, NOT the last request — the same trap the
/// xdg-decoration binding hit ("opcode 1 = get_toplevel_decoration (opcode 0 is
/// `destroy`!)").
pub fn ext_image_copy_capture_frame_v1_interface() -> &'static defines::wl_interface {
    use std::sync::OnceLock;
    static I: OnceLock<defines::SyncInterface> = OnceLock::new();
    I.get_or_init(|| {
        defines::SyncInterface({
            let nt = leak_null_types!(4);
            let requests: &'static [defines::wl_message] = Box::leak(Box::new([
                defines::wl_message {
                    name: b"destroy\0".as_ptr() as _,
                    signature: b"\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
                defines::wl_message {
                    name: b"attach_buffer\0".as_ptr() as _,
                    signature: b"o\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
                defines::wl_message {
                    name: b"damage_buffer\0".as_ptr() as _,
                    signature: b"iiii\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
                defines::wl_message {
                    name: b"capture\0".as_ptr() as _,
                    signature: b"\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
            ]));
            let events: &'static [defines::wl_message] = Box::leak(Box::new([
                defines::wl_message {
                    name: b"transform\0".as_ptr() as _,
                    signature: b"u\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
                defines::wl_message {
                    name: b"damage\0".as_ptr() as _,
                    signature: b"iiii\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
                defines::wl_message {
                    name: b"presentation_time\0".as_ptr() as _,
                    signature: b"uuu\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
                defines::wl_message {
                    name: b"ready\0".as_ptr() as _,
                    signature: b"\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
                defines::wl_message {
                    name: b"failed\0".as_ptr() as _,
                    signature: b"u\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
            ]));
            Box::leak(Box::new(defines::wl_interface {
                name: b"ext_image_copy_capture_frame_v1\0".as_ptr() as _,
                version: 1,
                method_count: 4,
                methods: requests.as_ptr(),
                event_count: 5,
                events: events.as_ptr(),
            }))
        })
    })
    .0
}

/// `ext_foreign_toplevel_handle_v1`.
/// Requests: destroy = 0.
/// Events: closed = 0, done = 1, title(s) = 2, app_id(s) = 3, identifier(s) = 4.
pub fn ext_foreign_toplevel_handle_v1_interface() -> &'static defines::wl_interface {
    use std::sync::OnceLock;
    static I: OnceLock<defines::SyncInterface> = OnceLock::new();
    I.get_or_init(|| {
        defines::SyncInterface({
            let nt = leak_null_types!(4);
            let requests: &'static [defines::wl_message] =
                Box::leak(Box::new([defines::wl_message {
                    name: b"destroy\0".as_ptr() as _,
                    signature: b"\0".as_ptr() as _,
                    types: nt.as_ptr(),
                }]));
            let events: &'static [defines::wl_message] = Box::leak(Box::new([
                defines::wl_message {
                    name: b"closed\0".as_ptr() as _,
                    signature: b"\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
                defines::wl_message {
                    name: b"done\0".as_ptr() as _,
                    signature: b"\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
                defines::wl_message {
                    name: b"title\0".as_ptr() as _,
                    signature: b"s\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
                defines::wl_message {
                    name: b"app_id\0".as_ptr() as _,
                    signature: b"s\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
                defines::wl_message {
                    name: b"identifier\0".as_ptr() as _,
                    signature: b"s\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
            ]));
            Box::leak(Box::new(defines::wl_interface {
                name: b"ext_foreign_toplevel_handle_v1\0".as_ptr() as _,
                version: 1,
                method_count: 1,
                methods: requests.as_ptr(),
                event_count: 5,
                events: events.as_ptr(),
            }))
        })
    })
    .0
}

/// `ext_foreign_toplevel_list_v1`.
/// Requests: stop = 0, destroy = 1.
/// Events: toplevel(new_id<handle>) = 0, finished = 1.
///
/// THE ONE TABLE WHOSE `types` IS LOAD-BEARING: the `toplevel` event carries a
/// `new_id`, which libwayland constructs itself, and it reads the interface to
/// construct out of `types[0]`. A null there dereferences inside libwayland.
pub fn ext_foreign_toplevel_list_v1_interface() -> &'static defines::wl_interface {
    use std::sync::OnceLock;
    static I: OnceLock<defines::SyncInterface> = OnceLock::new();
    I.get_or_init(|| {
        defines::SyncInterface({
            let nt = leak_null_types!(4);
            let handle_types: &'static [*const defines::wl_interface] = Box::leak(Box::new([
                ext_foreign_toplevel_handle_v1_interface() as *const defines::wl_interface,
            ]));
            let requests: &'static [defines::wl_message] = Box::leak(Box::new([
                defines::wl_message {
                    name: b"stop\0".as_ptr() as _,
                    signature: b"\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
                defines::wl_message {
                    name: b"destroy\0".as_ptr() as _,
                    signature: b"\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
            ]));
            let events: &'static [defines::wl_message] = Box::leak(Box::new([
                defines::wl_message {
                    name: b"toplevel\0".as_ptr() as _,
                    signature: b"n\0".as_ptr() as _,
                    types: handle_types.as_ptr(),
                },
                defines::wl_message {
                    name: b"finished\0".as_ptr() as _,
                    signature: b"\0".as_ptr() as _,
                    types: nt.as_ptr(),
                },
            ]));
            Box::leak(Box::new(defines::wl_interface {
                name: b"ext_foreign_toplevel_list_v1\0".as_ptr() as _,
                version: 1,
                method_count: 2,
                methods: requests.as_ptr(),
                event_count: 2,
                events: events.as_ptr(),
            }))
        })
    })
    .0
}

/// `wl_shm.format.xrgb8888`. Only ARGB8888 is defined in `defines`; the two
/// are the formats every compositor must support.
const WL_SHM_FORMAT_XRGB8888: u32 = 1;

// ── Capture state ───────────────────────────────────────────────────────────

struct Toplevel {
    handle: *mut defines::wl_proxy,
    title: String,
    app_id: String,
}

#[derive(Default)]
struct Caps {
    shm: *mut c_void,
    toplevel_list: *mut c_void,
    source_manager: *mut c_void,
    capture_manager: *mut c_void,
}

struct State {
    /// The loaded libwayland, for the C callbacks — they receive only `data`.
    wl: *const Wayland,
    caps: Caps,
    toplevels: Vec<Toplevel>,
    /// Session negotiation, filled by the session listener before `done`.
    width: u32,
    height: u32,
    shm_format: Option<u32>,
    session_done: bool,
    session_stopped: bool,
    /// Frame outcome.
    frame_ready: bool,
    frame_failed: Option<u32>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            wl: core::ptr::null(),
            caps: Caps::default(),
            toplevels: Vec::new(),
            width: 0,
            height: 0,
            shm_format: None,
            session_done: false,
            session_stopped: false,
            frame_ready: false,
            frame_failed: None,
        }
    }
}

// ── Listeners ───────────────────────────────────────────────────────────────
//
// A listener is an array of function pointers in EVENT DECLARATION ORDER. Every
// event needs an entry even when it is ignored: libwayland indexes this by
// opcode and calls whatever sits at that slot.

#[repr(C)]
struct RegistryListener {
    global: unsafe extern "C" fn(*mut c_void, *mut c_void, u32, *const c_char, u32),
    global_remove: unsafe extern "C" fn(*mut c_void, *mut c_void, u32),
}

#[repr(C)]
struct ToplevelListListener {
    toplevel: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut defines::wl_proxy),
    finished: unsafe extern "C" fn(*mut c_void, *mut c_void),
}

#[repr(C)]
struct ToplevelHandleListener {
    closed: unsafe extern "C" fn(*mut c_void, *mut c_void),
    done: unsafe extern "C" fn(*mut c_void, *mut c_void),
    title: unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_char),
    app_id: unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_char),
    identifier: unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_char),
}

#[repr(C)]
struct SessionListener {
    buffer_size: unsafe extern "C" fn(*mut c_void, *mut c_void, u32, u32),
    shm_format: unsafe extern "C" fn(*mut c_void, *mut c_void, u32),
    dmabuf_device: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void),
    dmabuf_format: unsafe extern "C" fn(*mut c_void, *mut c_void, u32, *mut c_void),
    done: unsafe extern "C" fn(*mut c_void, *mut c_void),
    stopped: unsafe extern "C" fn(*mut c_void, *mut c_void),
}

#[repr(C)]
struct FrameListener {
    transform: unsafe extern "C" fn(*mut c_void, *mut c_void, u32),
    damage: unsafe extern "C" fn(*mut c_void, *mut c_void, i32, i32, i32, i32),
    presentation_time: unsafe extern "C" fn(*mut c_void, *mut c_void, u32, u32, u32),
    ready: unsafe extern "C" fn(*mut c_void, *mut c_void),
    failed: unsafe extern "C" fn(*mut c_void, *mut c_void, u32),
}

unsafe fn state<'a>(data: *mut c_void) -> &'a mut State {
    &mut *(data as *mut State)
}

unsafe fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}

unsafe extern "C" fn on_global(
    data: *mut c_void,
    registry: *mut c_void,
    name: u32,
    interface: *const c_char,
    version: u32,
) {
    let st = state(data);
    let iface = cstr(interface);
    if st.wl.is_null() {
        return;
    }
    let wl = &*st.wl;
    let mut bind = |target: &'static defines::wl_interface, ver: u32| -> *mut c_void {
        (wl.wl_registry_bind)(
            registry as *mut defines::wl_registry,
            name,
            target,
            ver.min(version),
        )
    };
    match iface.as_str() {
        "wl_shm" => st.caps.shm = bind(&wl.wl_shm_interface, 1),
        "ext_foreign_toplevel_list_v1" => {
            st.caps.toplevel_list = bind(ext_foreign_toplevel_list_v1_interface(), 1);
        }
        "ext_foreign_toplevel_image_capture_source_manager_v1" => {
            st.caps.source_manager =
                bind(ext_foreign_toplevel_image_capture_source_manager_v1_interface(), 1);
        }
        "ext_image_copy_capture_manager_v1" => {
            st.caps.capture_manager = bind(ext_image_copy_capture_manager_v1_interface(), 1);
        }
        _ => {}
    }
}

unsafe extern "C" fn on_global_remove(_: *mut c_void, _: *mut c_void, _: u32) {}

unsafe extern "C" fn on_toplevel(
    data: *mut c_void,
    _list: *mut c_void,
    handle: *mut defines::wl_proxy,
) {
    let st = state(data);
    // The handle arrives before its title/app_id: attach a listener so the
    // strings land in the entry we just pushed.
    st.toplevels.push(Toplevel {
        handle,
        title: String::new(),
        app_id: String::new(),
    });
    let idx = st.toplevels.len() - 1;
    if !st.wl.is_null() {
        let wl = &*st.wl;
        static HANDLE_LISTENER: ToplevelHandleListener = ToplevelHandleListener {
            closed: on_handle_closed,
            done: on_handle_done,
            title: on_handle_title,
            app_id: on_handle_app_id,
            identifier: on_handle_identifier,
        };
        // The per-handle user data is (state, index), boxed and leaked for the
        // lifetime of the capture — the whole connection is torn down at the
        // end of `capture_toplevel`, so nothing outlives it.
        let ctx = Box::into_raw(Box::new((data, idx)));
        (wl.wl_proxy_add_listener)(
            handle,
            &HANDLE_LISTENER as *const _ as *const c_void,
            ctx as *mut c_void,
        );
    }
}

unsafe fn handle_entry<'a>(data: *mut c_void) -> Option<&'a mut Toplevel> {
    let (st_ptr, idx) = *(data as *mut (*mut c_void, usize));
    state(st_ptr).toplevels.get_mut(idx)
}

unsafe extern "C" fn on_handle_closed(_: *mut c_void, _: *mut c_void) {}
unsafe extern "C" fn on_handle_done(_: *mut c_void, _: *mut c_void) {}
unsafe extern "C" fn on_handle_title(data: *mut c_void, _: *mut c_void, title: *const c_char) {
    if let Some(t) = handle_entry(data) {
        t.title = cstr(title);
    }
}
unsafe extern "C" fn on_handle_app_id(data: *mut c_void, _: *mut c_void, app_id: *const c_char) {
    if let Some(t) = handle_entry(data) {
        t.app_id = cstr(app_id);
    }
}
unsafe extern "C" fn on_handle_identifier(_: *mut c_void, _: *mut c_void, _: *const c_char) {}
unsafe extern "C" fn on_list_finished(_: *mut c_void, _: *mut c_void) {}

unsafe extern "C" fn on_buffer_size(data: *mut c_void, _: *mut c_void, w: u32, h: u32) {
    let st = state(data);
    st.width = w;
    st.height = h;
}
unsafe extern "C" fn on_shm_format(data: *mut c_void, _: *mut c_void, format: u32) {
    let st = state(data);
    // Take the first format offered that we can actually convert. Both are
    // 32-bit little-endian; the difference is the channel order.
    if st.shm_format.is_none()
        && (format == defines::WL_SHM_FORMAT_ARGB8888 || format == WL_SHM_FORMAT_XRGB8888)
    {
        st.shm_format = Some(format);
    }
}
unsafe extern "C" fn on_dmabuf_device(_: *mut c_void, _: *mut c_void, _: *mut c_void) {}
unsafe extern "C" fn on_dmabuf_format(_: *mut c_void, _: *mut c_void, _: u32, _: *mut c_void) {}
unsafe extern "C" fn on_session_done(data: *mut c_void, _: *mut c_void) {
    state(data).session_done = true;
}
unsafe extern "C" fn on_session_stopped(data: *mut c_void, _: *mut c_void) {
    state(data).session_stopped = true;
}

unsafe extern "C" fn on_frame_transform(_: *mut c_void, _: *mut c_void, _: u32) {}
unsafe extern "C" fn on_frame_damage(
    _: *mut c_void,
    _: *mut c_void,
    _: i32,
    _: i32,
    _: i32,
    _: i32,
) {
}
unsafe extern "C" fn on_frame_presentation_time(
    _: *mut c_void,
    _: *mut c_void,
    _: u32,
    _: u32,
    _: u32,
) {
}
unsafe extern "C" fn on_frame_ready(data: *mut c_void, _: *mut c_void) {
    state(data).frame_ready = true;
}
unsafe extern "C" fn on_frame_failed(data: *mut c_void, _: *mut c_void, reason: u32) {
    state(data).frame_failed = Some(reason);
}

// ── The capture ─────────────────────────────────────────────────────────────

/// Capture the toplevel whose title is `title`, as the compositor composites
/// it — decorations included — and return it as PNG bytes.
///
/// Runs on its OWN `wl_display_connect`, not the application's. A capture is a
/// blocking round-trip conversation and the caller is inside a callback on the
/// app's event loop; borrowing that loop's queue to run it would re-enter the
/// app's own event dispatch. A second connection costs one socket and is
/// impossible to get wrong in that way.
pub fn capture_toplevel(title: &str) -> Result<Vec<u8>, AzString> {
    let wl_rc = Wayland::new()
        .map_err(|e| AzString::from(alloc::format!("libwayland-client: {e:?}")))?;
    let wl: &Wayland = &wl_rc;

    unsafe {
        let display = (wl.wl_display_connect)(core::ptr::null());
        if display.is_null() {
            return Err(AzString::from("wl_display_connect failed"));
        }
        // Everything below is fallible; one guard tears the connection down on
        // every path out.
        struct Conn<'a>(&'a Wayland, *mut defines::wl_display);
        impl Drop for Conn<'_> {
            fn drop(&mut self) {
                unsafe { (self.0.wl_display_disconnect)(self.1) };
            }
        }
        let _conn = Conn(wl, display);

        let mut st = State::default();
        st.wl = wl as *const Wayland;
        let st_ptr = &mut st as *mut State as *mut c_void;

        let registry = (wl.wl_display_get_registry)(display);
        static REGISTRY_LISTENER: RegistryListener = RegistryListener {
            global: on_global,
            global_remove: on_global_remove,
        };
        (wl.wl_proxy_add_listener)(
            registry as *mut defines::wl_proxy,
            &REGISTRY_LISTENER as *const _ as *const c_void,
            st_ptr,
        );
        (wl.wl_display_roundtrip)(display);

        if st.caps.capture_manager.is_null() {
            return Err(AzString::from(
                "compositor does not advertise ext_image_copy_capture_manager_v1 — a Wayland \
                 window capture needs it (wlroots' older wlr-screencopy is not implemented here)",
            ));
        }
        if st.caps.toplevel_list.is_null() || st.caps.source_manager.is_null() {
            return Err(AzString::from(
                "compositor does not advertise ext_foreign_toplevel_list_v1 + \
                 ext_foreign_toplevel_image_capture_source_manager_v1 — without them there is no \
                 way to name a WINDOW as the capture source",
            ));
        }

        // ── Find our own toplevel ───────────────────────────────────────
        static LIST_LISTENER: ToplevelListListener = ToplevelListListener {
            toplevel: on_toplevel,
            finished: on_list_finished,
        };
        (wl.wl_proxy_add_listener)(
            st.caps.toplevel_list as *mut defines::wl_proxy,
            &LIST_LISTENER as *const _ as *const c_void,
            st_ptr,
        );
        // Two round trips: the first delivers the `toplevel` events, the second
        // the `title`/`app_id` that follow on each handle.
        (wl.wl_display_roundtrip)(display);
        (wl.wl_display_roundtrip)(display);

        let matches: Vec<&Toplevel> = st.toplevels.iter().filter(|t| t.title == title).collect();
        let handle = match matches.len() {
            0 => {
                return Err(AzString::from(alloc::format!(
                    "no toplevel titled {title:?} in ext_foreign_toplevel_list_v1 (saw {})",
                    st.toplevels.len()
                )));
            }
            1 => matches[0].handle,
            n => {
                // The protocol offers no way to say "the one that is mine".
                return Err(AzString::from(alloc::format!(
                    "{n} toplevels are titled {title:?}; ext-foreign-toplevel-list gives no way \
                     to tell which is this process's window"
                )));
            }
        };

        // ── source -> session ───────────────────────────────────────────
        type CreateSourceFn = unsafe extern "C" fn(
            *mut defines::wl_proxy,
            u32,
            *const defines::wl_interface,
            *mut c_void,
            *mut defines::wl_proxy,
        ) -> *mut defines::wl_proxy;
        let create_source: CreateSourceFn = core::mem::transmute(wl.wl_proxy_marshal_constructor);
        let source = create_source(
            st.caps.source_manager as *mut defines::wl_proxy,
            0, // create_source
            ext_image_capture_source_v1_interface(),
            core::ptr::null_mut(),
            handle,
        );
        if source.is_null() {
            return Err(AzString::from("create_source returned NULL"));
        }

        type CreateSessionFn = unsafe extern "C" fn(
            *mut defines::wl_proxy,
            u32,
            *const defines::wl_interface,
            *mut c_void,
            *mut defines::wl_proxy,
            u32,
        ) -> *mut defines::wl_proxy;
        let create_session: CreateSessionFn = core::mem::transmute(wl.wl_proxy_marshal_constructor);
        let session = create_session(
            st.caps.capture_manager as *mut defines::wl_proxy,
            0, // create_session
            ext_image_copy_capture_session_v1_interface(),
            core::ptr::null_mut(),
            source,
            0, // options: no pointer cursor
        );
        if session.is_null() {
            return Err(AzString::from("create_session returned NULL"));
        }

        static SESSION_LISTENER: SessionListener = SessionListener {
            buffer_size: on_buffer_size,
            shm_format: on_shm_format,
            dmabuf_device: on_dmabuf_device,
            dmabuf_format: on_dmabuf_format,
            done: on_session_done,
            stopped: on_session_stopped,
        };
        (wl.wl_proxy_add_listener)(
            session,
            &SESSION_LISTENER as *const _ as *const c_void,
            st_ptr,
        );

        // The session announces its buffer constraints and ends with `done`.
        for _ in 0..16 {
            if st.session_done || st.session_stopped {
                break;
            }
            (wl.wl_display_roundtrip)(display);
        }
        if st.session_stopped {
            return Err(AzString::from("capture session stopped before it was usable"));
        }
        if !st.session_done {
            return Err(AzString::from("capture session never sent `done`"));
        }
        let (w, h) = (st.width, st.height);
        if w == 0 || h == 0 {
            return Err(AzString::from("capture session announced a zero buffer size"));
        }
        let format = st.shm_format.ok_or_else(|| {
            AzString::from(
                "capture session offered no shm format this code can convert (only ARGB8888 / \
                 XRGB8888; a dmabuf-only session is not implemented)",
            )
        })?;

        // ── destination buffer ──────────────────────────────────────────
        let stride = (w * 4) as i32;
        let size = stride * h as i32;
        let fd = memfd(size)?;
        let data = libc::mmap(
            core::ptr::null_mut(),
            size as usize,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd,
            0,
        );
        if data == libc::MAP_FAILED {
            libc::close(fd);
            return Err(AzString::from("mmap of the capture buffer failed"));
        }
        let pool = (wl.wl_shm_create_pool)(st.caps.shm as *mut defines::wl_shm, fd, size);
        libc::close(fd);
        let buffer =
            (wl.wl_shm_pool_create_buffer)(pool, 0, w as i32, h as i32, stride, format);

        // ── frame ───────────────────────────────────────────────────────
        type CreateFrameFn = unsafe extern "C" fn(
            *mut defines::wl_proxy,
            u32,
            *const defines::wl_interface,
            *mut c_void,
        ) -> *mut defines::wl_proxy;
        let create_frame: CreateFrameFn = core::mem::transmute(wl.wl_proxy_marshal_constructor);
        let frame = create_frame(
            session,
            0, // create_frame
            ext_image_copy_capture_frame_v1_interface(),
            core::ptr::null_mut(),
        );
        if frame.is_null() {
            (wl.wl_buffer_destroy)(buffer);
            (wl.wl_shm_pool_destroy)(pool);
            libc::munmap(data, size as usize);
            return Err(AzString::from("create_frame returned NULL"));
        }

        static FRAME_LISTENER: FrameListener = FrameListener {
            transform: on_frame_transform,
            damage: on_frame_damage,
            presentation_time: on_frame_presentation_time,
            ready: on_frame_ready,
            failed: on_frame_failed,
        };
        (wl.wl_proxy_add_listener)(frame, &FRAME_LISTENER as *const _ as *const c_void, st_ptr);

        type MarshalObjFn = unsafe extern "C" fn(*mut defines::wl_proxy, u32, *mut c_void);
        type Marshal4iFn = unsafe extern "C" fn(*mut defines::wl_proxy, u32, i32, i32, i32, i32);
        type Marshal0Fn = unsafe extern "C" fn(*mut defines::wl_proxy, u32);
        let marshal_obj: MarshalObjFn = core::mem::transmute(wl.wl_proxy_marshal);
        let marshal_4i: Marshal4iFn = core::mem::transmute(wl.wl_proxy_marshal);
        let marshal_0: Marshal0Fn = core::mem::transmute(wl.wl_proxy_marshal);

        marshal_obj(frame, 1, buffer as *mut c_void); // attach_buffer
        marshal_4i(frame, 2, 0, 0, w as i32, h as i32); // damage_buffer
        marshal_0(frame, 3); // capture

        for _ in 0..240 {
            if st.frame_ready || st.frame_failed.is_some() {
                break;
            }
            if (wl.wl_display_roundtrip)(display) < 0 {
                break;
            }
        }

        let outcome = if let Some(reason) = st.frame_failed {
            // 1 = unknown, 2 = buffer constraints, 3 = stopped (protocol enum).
            Err(AzString::from(alloc::format!(
                "capture frame failed (reason {reason})"
            )))
        } else if !st.frame_ready {
            Err(AzString::from("capture frame never became ready"))
        } else {
            // ARGB8888 / XRGB8888 are little-endian 32-bit: B,G,R,A in memory.
            let src = core::slice::from_raw_parts(data as *const u8, size as usize);
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for y in 0..h as usize {
                let row = &src[y * stride as usize..y * stride as usize + (w * 4) as usize];
                for px in row.chunks_exact(4) {
                    rgba.push(px[2]); // R
                    rgba.push(px[1]); // G
                    rgba.push(px[0]); // B
                    // XRGB has no alpha channel; its top byte is padding.
                    rgba.push(if format == defines::WL_SHM_FORMAT_ARGB8888 {
                        px[3]
                    } else {
                        255
                    });
                }
            }
            crate::desktop::native_screenshot::encode_rgba_png(rgba, w, h).map_err(AzString::from)
        };

        marshal_0(frame, 0); // frame.destroy
        (wl.wl_buffer_destroy)(buffer);
        (wl.wl_shm_pool_destroy)(pool);
        libc::munmap(data, size as usize);
        marshal_0(session, 1); // session.destroy
        marshal_0(source, 0); // source.destroy
        (wl.wl_display_flush)(display);

        outcome
    }
}

/// An anonymous shared-memory file for the destination buffer. Same
/// memfd-then-shm_open fallback as the window backbuffer in `mod.rs`.
unsafe fn memfd(size: i32) -> Result<libc::c_int, AzString> {
    use std::ffi::CString;
    let name = CString::new("azul-capture").unwrap();
    let mut fd = libc::syscall(libc::SYS_memfd_create, name.as_ptr(), 1 as libc::c_int)
        as libc::c_int;
    if fd == -1 {
        let name = CString::new(alloc::format!("/azul-capture-{}", std::process::id())).unwrap();
        fd = libc::shm_open(
            name.as_ptr(),
            libc::O_CREAT | libc::O_RDWR | libc::O_EXCL,
            0o600,
        );
        if fd != -1 {
            libc::shm_unlink(name.as_ptr());
        }
    }
    if fd == -1 {
        return Err(AzString::from("could not create shared memory for the capture"));
    }
    if libc::ftruncate(fd, size as libc::off_t) == -1 {
        libc::close(fd);
        return Err(AzString::from("ftruncate of the capture buffer failed"));
    }
    Ok(fd)
}
