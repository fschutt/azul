//! X11 global hotkeys: `XGrabKey` on the root window.
//!
//! # A connection of its own
//!
//! The grabs live on a SECOND display connection, opened at the first grab
//! and closed with the backend. A grab's `KeyPress` is reported to the
//! grabbing client with `window = root`; on the windows' shared connection it
//! would reach an event router that knows only its own windows. The cost is
//! that this connection's fd is not in the loop's poll set: the backend
//! exposes it ([`GlobalHotkeyBackend::wake_fd`]) for a loop that can add it,
//! and until one does, the X11 / Wayland loops cap their park while a hotkey
//! is grabbed (like they do for the tray's D-Bus) and [`X11Backend::poll`]
//! drains it on every loop iteration.
//!
//! # The lock modifiers
//!
//! A grab matches the modifier state EXACTLY, and Caps Lock, Num Lock and
//! (where it is a modifier) Scroll Lock are modifier bits too - a Ctrl+Alt+K
//! grab does not fire while Num Lock is on. So every combination is grabbed
//! once per subset of the lock masks present on this keyboard (Num Lock's
//! and Scroll Lock's bits are read from the modifier mapping, not assumed),
//! and a press is matched with those bits masked off.
//!
//! # BadAccess
//!
//! A combination another client already grabbed fails ASYNCHRONOUSLY with a
//! `BadAccess` error, which Xlib hands to the process-wide error handler -
//! whose default prints and EXITS. So the grabs run under a handler of our
//! own, then `XSync` collects the verdict, then the previous handler is put
//! back. `BadAccess` = `TakenByAnotherApp`; every grab of the combination is
//! then released again, so a half-grabbed chord is never left behind. The
//! handler has no user pointer, so the verdict goes through two statics that
//! only live for the duration of one grab.
//!
//! libX11 is loaded here with `libloading` rather than through the shell's
//! `Xlib` table, which does not carry the grab calls.

use std::{
    ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void, CString},
    sync::{
        atomic::{AtomicU8, AtomicUsize, Ordering},
        OnceLock,
    },
};

use azul_core::global_hotkey::{xkb_keysym_name, GlobalHotkey, GlobalHotkeyError, GlobalHotkeyId};
use azul_layout::managers::global_hotkey::{BackendGrant, GlobalHotkeyBackend, HotkeySink};

type Display = c_void;
type Window = c_ulong;
type KeySym = c_ulong;

/// `XErrorEvent`.
#[repr(C)]
struct XErrorEvent {
    kind: c_int,
    display: *mut Display,
    resource_id: c_ulong,
    serial: c_ulong,
    error_code: u8,
    request_code: u8,
    minor_code: u8,
}

/// `XKeyEvent`, the prefix of the `XEvent` union a key press is read through.
#[repr(C)]
struct XKeyEvent {
    kind: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut Display,
    window: Window,
    root: Window,
    subwindow: Window,
    time: c_ulong,
    x: c_int,
    y: c_int,
    x_root: c_int,
    y_root: c_int,
    state: c_uint,
    keycode: c_uint,
    same_screen: c_int,
}

/// `XEvent`: a union of 24 longs.
#[repr(C)]
struct XEvent {
    pad: [c_long; 24],
}

/// `XModifierKeymap`.
#[repr(C)]
struct XModifierKeymap {
    max_keypermod: c_int,
    modifiermap: *mut u8,
}

type XErrorHandler = unsafe extern "C" fn(*mut Display, *mut XErrorEvent) -> c_int;

const KEY_PRESS: c_int = 2;
const KEY_RELEASE: c_int = 3;
const BAD_ACCESS: u8 = 10;
const GRAB_MODE_ASYNC: c_int = 1;

const SHIFT_MASK: c_uint = 1 << 0;
const LOCK_MASK: c_uint = 1 << 1;
const CONTROL_MASK: c_uint = 1 << 2;
/// Alt, on every mainstream keymap.
const MOD1_MASK: c_uint = 1 << 3;
/// Num Lock's usual bit, used when the modifier mapping cannot be read.
const MOD2_MASK: c_uint = 1 << 4;
/// Super, on every mainstream keymap.
const MOD4_MASK: c_uint = 1 << 6;
/// The bits a hotkey is made of; everything else (the lock bits, AltGr's
/// Mod5, button bits) is ignored when matching a press.
const HOTKEY_MASK: c_uint = SHIFT_MASK | CONTROL_MASK | MOD1_MASK | MOD4_MASK;

const XK_NUM_LOCK: KeySym = 0xff7f;
const XK_SCROLL_LOCK: KeySym = 0xff14;

/// What the capability probe and the backend call themselves.
pub(super) const NAME: &str = "X11 XGrabKey (root window)";

type XOpenDisplayFn = unsafe extern "C" fn(*const c_char) -> *mut Display;
type XCloseDisplayFn = unsafe extern "C" fn(*mut Display) -> c_int;
type XConnectionNumberFn = unsafe extern "C" fn(*mut Display) -> c_int;
type XDefaultRootWindowFn = unsafe extern "C" fn(*mut Display) -> Window;
type XGrabKeyFn =
    unsafe extern "C" fn(*mut Display, c_int, c_uint, Window, c_int, c_int, c_int) -> c_int;
type XUngrabKeyFn = unsafe extern "C" fn(*mut Display, c_int, c_uint, Window) -> c_int;
type XKeysymToKeycodeFn = unsafe extern "C" fn(*mut Display, KeySym) -> u8;
type XStringToKeysymFn = unsafe extern "C" fn(*const c_char) -> KeySym;
type XSetErrorHandlerFn = unsafe extern "C" fn(Option<XErrorHandler>) -> Option<XErrorHandler>;
type XSyncFn = unsafe extern "C" fn(*mut Display, c_int) -> c_int;
type XPendingFn = unsafe extern "C" fn(*mut Display) -> c_int;
type XConnectionNumberFn = unsafe extern "C" fn(*mut Display) -> c_int;
type XNextEventFn = unsafe extern "C" fn(*mut Display, *mut XEvent) -> c_int;
type XGetModifierMappingFn = unsafe extern "C" fn(*mut Display) -> *mut XModifierKeymap;
type XFreeModifiermapFn = unsafe extern "C" fn(*mut XModifierKeymap) -> c_int;
type XkbSetDetectableAutoRepeatFn = unsafe extern "C" fn(*mut Display, c_int, *mut c_int) -> c_int;

struct Xlib {
    open_display: XOpenDisplayFn,
    close_display: XCloseDisplayFn,
    connection_number: XConnectionNumberFn,
    default_root_window: XDefaultRootWindowFn,
    grab_key: XGrabKeyFn,
    ungrab_key: XUngrabKeyFn,
    keysym_to_keycode: XKeysymToKeycodeFn,
    string_to_keysym: XStringToKeysymFn,
    set_error_handler: XSetErrorHandlerFn,
    sync: XSyncFn,
    pending: XPendingFn,
    connection_number: XConnectionNumberFn,
    next_event: XNextEventFn,
    get_modifier_mapping: XGetModifierMappingFn,
    free_modifiermap: XFreeModifiermapFn,
    /// Optional: without it a held chord auto-repeats as release/press pairs.
    set_detectable_auto_repeat: Option<XkbSetDetectableAutoRepeatFn>,
}

fn xlib() -> Option<&'static Xlib> {
    static XLIB: OnceLock<Option<Xlib>> = OnceLock::new();
    XLIB.get_or_init(|| unsafe {
        let lib = libloading::Library::new("libX11.so.6")
            .or_else(|_| libloading::Library::new("libX11.so"))
            .ok()?;
        let fns = Xlib {
            open_display: *lib.get::<XOpenDisplayFn>(b"XOpenDisplay\0").ok()?,
            close_display: *lib.get::<XCloseDisplayFn>(b"XCloseDisplay\0").ok()?,
            connection_number: *lib
                .get::<XConnectionNumberFn>(b"XConnectionNumber\0")
                .ok()?,
            default_root_window: *lib
                .get::<XDefaultRootWindowFn>(b"XDefaultRootWindow\0")
                .ok()?,
            grab_key: *lib.get::<XGrabKeyFn>(b"XGrabKey\0").ok()?,
            ungrab_key: *lib.get::<XUngrabKeyFn>(b"XUngrabKey\0").ok()?,
            keysym_to_keycode: *lib
                .get::<XKeysymToKeycodeFn>(b"XKeysymToKeycode\0")
                .ok()?,
            string_to_keysym: *lib
                .get::<XStringToKeysymFn>(b"XStringToKeysym\0")
                .ok()?,
            set_error_handler: *lib
                .get::<XSetErrorHandlerFn>(b"XSetErrorHandler\0")
                .ok()?,
            sync: *lib.get::<XSyncFn>(b"XSync\0").ok()?,
            pending: *lib.get::<XPendingFn>(b"XPending\0").ok()?,
            connection_number: *lib
                .get::<XConnectionNumberFn>(b"XConnectionNumber\0")
                .ok()?,
            next_event: *lib.get::<XNextEventFn>(b"XNextEvent\0").ok()?,
            get_modifier_mapping: *lib
                .get::<XGetModifierMappingFn>(b"XGetModifierMapping\0")
                .ok()?,
            free_modifiermap: *lib
                .get::<XFreeModifiermapFn>(b"XFreeModifiermap\0")
                .ok()?,
            set_detectable_auto_repeat: lib
                .get::<XkbSetDetectableAutoRepeatFn>(b"XkbSetDetectableAutoRepeat\0")
                .ok()
                .map(|symbol| *symbol),
        };
        // libX11 stays loaded for the process (the windows use it too).
        std::mem::forget(lib);
        Some(fns)
    })
    .as_ref()
}

/// One grabbed combination.
struct Grab {
    os_id: u32,
    keycode: u8,
    /// The hotkey's own modifier bits (no lock bits).
    modifiers: c_uint,
}

/// The grab connection and what is grabbed on it. The display pointer is a
/// `usize` so the backend is `Send`; only the event-loop thread touches it.
struct Connection {
    display: usize,
    root: Window,
    /// Every subset of the lock masks present on this keyboard, `0` first.
    lock_combinations: Vec<c_uint>,
    /// Their union, masked off a press before matching.
    lock_bits: c_uint,
}

/// The error code the grab handler saw on OUR display (0 = none).
static GRAB_ERROR: AtomicU8 = AtomicU8::new(0);
/// The display of the grab in progress, so the handler only judges errors
/// that are ours.
static GRAB_DISPLAY: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn grab_error_handler(display: *mut Display, event: *mut XErrorEvent) -> c_int {
    if !event.is_null() && display as usize == GRAB_DISPLAY.load(Ordering::Relaxed) {
        let code = unsafe { (*event).error_code };
        // Keep the first error: BadAccess on one lock variant is the verdict.
        let _ = GRAB_ERROR.compare_exchange(0, code, Ordering::Relaxed, Ordering::Relaxed);
    }
    0
}

/// The mask bit a keysym's key is bound to in the modifier mapping, or 0.
fn modifier_bit_of(x: &Xlib, display: *mut Display, keysym: KeySym) -> c_uint {
    unsafe {
        let keycode = (x.keysym_to_keycode)(display, keysym);
        if keycode == 0 {
            return 0;
        }
        let map = (x.get_modifier_mapping)(display);
        if map.is_null() {
            return 0;
        }
        let per = usize::try_from((*map).max_keypermod).unwrap_or(0);
        let mut bit = 0;
        if !(*map).modifiermap.is_null() {
            'search: for modifier in 0..8_usize {
                for slot in 0..per {
                    if *(*map).modifiermap.add(modifier * per + slot) == keycode {
                        bit = 1 << modifier;
                        break 'search;
                    }
                }
            }
        }
        let _ = (x.free_modifiermap)(map);
        bit
    }
}

/// Open the grab connection.
fn open_connection(x: &Xlib) -> Result<Connection, GlobalHotkeyError> {
    let display = unsafe { (x.open_display)(core::ptr::null()) };
    if display.is_null() {
        return Err(GlobalHotkeyError::Unavailable(
            "could not open the X display ($DISPLAY)".into(),
        ));
    }
    let root = unsafe { (x.default_root_window)(display) };
    if let Some(detectable) = x.set_detectable_auto_repeat {
        let mut supported: c_int = 0;
        unsafe {
            let _ = detectable(display, 1, &mut supported);
        }
    }
    let num_lock = match modifier_bit_of(x, display, XK_NUM_LOCK) {
        0 => MOD2_MASK,
        bit => bit,
    };
    let scroll_lock = modifier_bit_of(x, display, XK_SCROLL_LOCK);
    let mut lock_masks: Vec<c_uint> = Vec::new();
    for mask in [LOCK_MASK, num_lock, scroll_lock] {
        if mask != 0 && !lock_masks.contains(&mask) {
            lock_masks.push(mask);
        }
    }
    let mut lock_combinations = Vec::new();
    for subset in 0..(1_u32 << lock_masks.len()) {
        let mut bits = 0;
        for (i, mask) in lock_masks.iter().enumerate() {
            if subset & (1 << i) != 0 {
                bits |= *mask;
            }
        }
        lock_combinations.push(bits);
    }
    let lock_bits = lock_masks.iter().fold(0, |acc, m| acc | *m);
    Ok(Connection {
        display: display as usize,
        root,
        lock_combinations,
        lock_bits,
    })
}

fn x11_modifiers(hotkey: &GlobalHotkey) -> c_uint {
    let m = hotkey.modifiers;
    let mut bits = 0;
    if m.shift {
        bits |= SHIFT_MASK;
    }
    if m.ctrl {
        bits |= CONTROL_MASK;
    }
    if m.alt {
        bits |= MOD1_MASK;
    }
    if m.meta {
        bits |= MOD4_MASK;
    }
    bits
}

/// Is there an X display and a libX11? No side effect beyond the (cached)
/// dlopen.
pub(super) fn probe() -> Result<(), String> {
    if std::env::var_os("DISPLAY").map_or(true, |d| d.is_empty()) {
        return Err(String::from("no X display ($DISPLAY is not set)"));
    }
    if xlib().is_none() {
        return Err(String::from("libX11 could not be loaded"));
    }
    Ok(())
}

/// One app's X11 grabs, on a connection of their own.
pub(super) struct X11Backend {
    sink: HotkeySink,
    connection: Option<Connection>,
    grabs: Vec<Grab>,
    /// Combinations currently held down, so an auto-repeated press (with
    /// detectable auto-repeat on) fires once.
    held: Vec<(u8, c_uint)>,
}

impl X11Backend {
    pub(super) fn new(sink: HotkeySink) -> Self {
        Self {
            sink,
            connection: None,
            grabs: Vec::new(),
            held: Vec::new(),
        }
    }
}

impl GlobalHotkeyBackend for X11Backend {
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
        _description: &str,
    ) -> Result<BackendGrant, GlobalHotkeyError> {
        let Some(x) = xlib() else {
            return Err(GlobalHotkeyError::Unavailable(
                "libX11 could not be loaded".into(),
            ));
        };
        let Some(name) = xkb_keysym_name(hotkey.key) else {
            return Err(GlobalHotkeyError::KeyNotMappable);
        };
        let Ok(name) = CString::new(name) else {
            return Err(GlobalHotkeyError::KeyNotMappable);
        };
        if self.connection.is_none() {
            self.connection = Some(open_connection(x)?);
        }
        let Some(c) = self.connection.as_ref() else {
            return Err(GlobalHotkeyError::Unavailable(
                "the X display closed".into(),
            ));
        };
        let display = c.display as *mut Display;
        let keysym = unsafe { (x.string_to_keysym)(name.as_ptr()) };
        let keycode = if keysym == 0 {
            0
        } else {
            unsafe { (x.keysym_to_keycode)(display, keysym) }
        };
        if keycode == 0 {
            // The keysym exists but no key on this keyboard produces it.
            return Err(GlobalHotkeyError::KeyNotMappable);
        }
        let modifiers = x11_modifiers(hotkey);

        // Grab every lock variant under our own error handler, then collect
        // the verdict with XSync before putting the previous handler back.
        GRAB_ERROR.store(0, Ordering::Relaxed);
        GRAB_DISPLAY.store(c.display, Ordering::Relaxed);
        let error = unsafe {
            let previous = (x.set_error_handler)(Some(grab_error_handler as XErrorHandler));
            for extra in &c.lock_combinations {
                let _ = (x.grab_key)(
                    display,
                    c_int::from(keycode),
                    modifiers | *extra,
                    c.root,
                    0,
                    GRAB_MODE_ASYNC,
                    GRAB_MODE_ASYNC,
                );
            }
            let _ = (x.sync)(display, 0);
            let _ = (x.set_error_handler)(previous);
            GRAB_ERROR.load(Ordering::Relaxed)
        };
        if error != 0 {
            // Release whatever part of the chord did take.
            unsafe {
                let previous = (x.set_error_handler)(Some(grab_error_handler as XErrorHandler));
                for extra in &c.lock_combinations {
                    let _ =
                        (x.ungrab_key)(display, c_int::from(keycode), modifiers | *extra, c.root);
                }
                let _ = (x.sync)(display, 0);
                let _ = (x.set_error_handler)(previous);
            }
            return Err(if error == BAD_ACCESS {
                GlobalHotkeyError::TakenByAnotherApp
            } else {
                GlobalHotkeyError::Platform(
                    format!("XGrabKey failed (X error code {error})").into(),
                )
            });
        }
        self.grabs.push(Grab {
            os_id: os_id.id,
            keycode,
            modifiers,
        });
        Ok(BackendGrant::Active)
    }

    fn unregister(&mut self, os_id: GlobalHotkeyId) {
        let Some(x) = xlib() else {
            return;
        };
        let Some(index) = self.grabs.iter().position(|g| g.os_id == os_id.id) else {
            return;
        };
        let grab = self.grabs.remove(index);
        self.held
            .retain(|(keycode, modifiers)| !(*keycode == grab.keycode && *modifiers == grab.modifiers));
        let Some(c) = self.connection.as_ref() else {
            return;
        };
        let display = c.display as *mut Display;
        unsafe {
            for extra in &c.lock_combinations {
                let _ = (x.ungrab_key)(
                    display,
                    c_int::from(grab.keycode),
                    grab.modifiers | *extra,
                    c.root,
                );
            }
            let _ = (x.sync)(display, 0);
        }
    }

    /// Drain the grab connection: every `KeyPress` that matches a grab parks
    /// a press in the sink. Non-blocking; called on every loop iteration.
    fn poll(&mut self) {
        let Some(x) = xlib() else {
            return;
        };
        let Some(c) = self.connection.as_ref() else {
            return;
        };
        let display = c.display as *mut Display;
        let lock_bits = c.lock_bits;
        let mut fired: Vec<(u32, u64)> = Vec::new();
        unsafe {
            while (x.pending)(display) > 0 {
                let mut event = XEvent { pad: [0; 24] };
                let _ = (x.next_event)(display, &mut event);
                let key = &*(&event as *const XEvent).cast::<XKeyEvent>();
                if key.kind != KEY_PRESS && key.kind != KEY_RELEASE {
                    continue;
                }
                let Ok(keycode) = u8::try_from(key.keycode) else {
                    continue;
                };
                let modifiers = key.state & !lock_bits & HOTKEY_MASK;
                let chord = (keycode, modifiers);
                if key.kind == KEY_RELEASE {
                    self.held.retain(|held| *held != chord);
                    continue;
                }
                if self.held.contains(&chord) {
                    // Auto-repeat of a chord still held down.
                    continue;
                }
                let grabbed = self
                    .grabs
                    .iter()
                    .find(|g| g.keycode == keycode && g.modifiers == modifiers)
                    .map(|g| g.os_id);
                if let Some(os_id) = grabbed {
                    self.held.push(chord);
                    fired.push((os_id, u64::from(key.time)));
                }
            }
        }
        for (id, time) in fired {
            self.sink
                .push(azul_layout::managers::global_hotkey::BackendEvent::Fired {
                    os_id: GlobalHotkeyId { id },
                    state: azul_core::global_hotkey::GlobalHotkeyState::Pressed,
                    timestamp_ms: time,
                });
        }
    }

    fn wake_fd(&self) -> Option<i32> {
        // The connection first: asked on every park, this must not load
        // libX11 in a session that never grabbed an X11 hotkey.
        let c = self.connection.as_ref()?;
        let x = xlib()?;
        let fd = unsafe { (x.connection_number)(c.display as *mut Display) };
        (fd >= 0).then_some(fd)
    }

    /// Events already in Xlib's queue for the grab connection? `XPending`
    /// flushes and reads whatever the socket holds without blocking, so
    /// after a `false` the socket is empty too and the fd announces the next
    /// press.
    fn has_buffered_input(&self) -> bool {
        let Some(c) = self.connection.as_ref() else {
            return false;
        };
        let Some(x) = xlib() else {
            return false;
        };
        unsafe { (x.pending)(c.display as *mut Display) > 0 }
    }

    fn needs_loop_polling(&self) -> bool {
        // Unless the loop watches `wake_fd` (it says so when it attaches its
        // waker), it has to wake up by itself to read the grab connection.
        true
    }
}

impl Drop for X11Backend {
    /// Dropping the App (or replacing the backend) releases every grab and
    /// closes the grab connection.
    fn drop(&mut self) {
        let Some(x) = xlib() else {
            return;
        };
        let Some(c) = self.connection.take() else {
            return;
        };
        let display = c.display as *mut Display;
        unsafe {
            for grab in &self.grabs {
                for extra in &c.lock_combinations {
                    let _ = (x.ungrab_key)(
                        display,
                        c_int::from(grab.keycode),
                        grab.modifiers | *extra,
                        c.root,
                    );
                }
            }
            let _ = (x.sync)(display, 0);
            let _ = (x.close_display)(display);
        }
    }
}
