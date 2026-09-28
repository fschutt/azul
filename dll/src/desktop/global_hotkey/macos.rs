//! macOS global hotkeys: Carbon's `RegisterEventHotKey` (HIToolbox).
//!
//! # Why Carbon, in 2026
//!
//! It is the only API for this that needs NO permission. The alternatives
//! both sit behind a TCC gate: a `CGEventTap` or `NSEvent`'s
//! `addGlobalMonitorForEventsMatchingMask:` need Accessibility / Input
//! Monitoring - the permission that lets an app read every keystroke
//! system-wide, which is a heavy ask for one shortcut, and a global monitor
//! cannot even CONSUME the key. `RegisterEventHotKey` is what Alfred,
//! Raycast, Rectangle and Electron's `globalShortcut` use.
//!
//! Most of Carbon is deprecated; the hot-key calls are not formally
//! deprecated in the HIToolbox headers (CarbonEvents.h), still ship in the
//! 64-bit HIToolbox on every macOS and have no Cocoa replacement. They are
//! dlopen'd rather than linked, like MediaPlayer beside them: a macOS that
//! ever drops them degrades to "unavailable" instead of failing to launch.
//!
//! # How a press reaches us
//!
//! The hot key arrives as an `NSEventTypeSystemDefined` event in the app's
//! queue; dispatching it (`[NSApp sendEvent:]`, which both run loops call)
//! runs the Carbon handler installed on the APPLICATION event target, on the
//! main thread. The handler only parks the id - the run loop's hotkey pump
//! runs the callback, outside AppKit's dispatch.
//!
//! # Exclusive
//!
//! Registered with `kEventHotKeyExclusive`, so a combination another process
//! holds exclusively fails with `eventHotKeyExistsErr` instead of silently
//! never firing - that is `TakenByAnotherApp`. System shortcuts owned by the
//! WindowServer (Cmd+Tab, Cmd+Space for Spotlight, ...) may still register
//! and simply never fire; macOS reports nothing for those.

use std::{
    collections::BTreeMap,
    ffi::{c_ulong, c_void},
    sync::{Mutex, OnceLock, PoisonError},
};

use azul_core::global_hotkey::{GlobalHotkey, GlobalHotkeyError, GlobalHotkeyId};
use azul_layout::managers::global_hotkey::{push_fired, BackendGrant, GlobalHotkeyBackend};

type OsStatus = i32;
type EventTargetRef = *mut c_void;
type EventRef = *mut c_void;
type EventHandlerCallRef = *mut c_void;
type EventHandlerRef = *mut c_void;
type EventHotKeyRef = *mut c_void;

/// `struct EventTypeSpec { OSType eventClass; UInt32 eventKind; }`
#[repr(C)]
struct EventTypeSpec {
    event_class: u32,
    event_kind: u32,
}

/// `struct EventHotKeyID { OSType signature; UInt32 id; }` - passed BY VALUE
/// to `RegisterEventHotKey`, 8 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
struct EventHotKeyId {
    signature: u32,
    id: u32,
}

type EventHandlerProc =
    unsafe extern "C" fn(EventHandlerCallRef, EventRef, *mut c_void) -> OsStatus;

/// `'keyb'`
const K_EVENT_CLASS_KEYBOARD: u32 = u32::from_be_bytes(*b"keyb");
/// `kEventHotKeyPressed`
const K_EVENT_HOT_KEY_PRESSED: u32 = 5;
/// `kEventParamDirectObject`, `'----'`
const K_EVENT_PARAM_DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");
/// `typeEventHotKeyID`, `'hkid'`
const TYPE_EVENT_HOT_KEY_ID: u32 = u32::from_be_bytes(*b"hkid");
/// `kEventHotKeyExclusive`
const K_EVENT_HOT_KEY_EXCLUSIVE: u32 = 1 << 0;
/// `eventHotKeyExistsErr`
const EVENT_HOT_KEY_EXISTS_ERR: OsStatus = -9878;
/// `eventHotKeyInvalidErr`
const EVENT_HOT_KEY_INVALID_ERR: OsStatus = -9879;
/// `eventNotHandledErr` - lets another handler see an event that is not ours.
const EVENT_NOT_HANDLED_ERR: OsStatus = -9874;
const NO_ERR: OsStatus = 0;

/// The `EventHotKeyID.signature` of every azul hot key, `'AZHK'`, so the
/// handler never claims a hot key another component of the process made.
const SIGNATURE: u32 = u32::from_be_bytes(*b"AZHK");

/// Carbon's modifier bits (`Events.h`).
const CMD_KEY: u32 = 1 << 8;
const SHIFT_KEY: u32 = 1 << 9;
const OPTION_KEY: u32 = 1 << 11;
const CONTROL_KEY: u32 = 1 << 12;

/// The HIToolbox entry points, resolved once. The library is leaked so the
/// fn pointers stay valid (HIToolbox never unloads anyway).
struct Carbon {
    get_application_event_target: unsafe extern "C" fn() -> EventTargetRef,
    install_event_handler: unsafe extern "C" fn(
        EventTargetRef,
        EventHandlerProc,
        c_ulong,
        *const EventTypeSpec,
        *mut c_void,
        *mut EventHandlerRef,
    ) -> OsStatus,
    register_event_hot_key: unsafe extern "C" fn(
        u32,
        u32,
        EventHotKeyId,
        EventTargetRef,
        u32,
        *mut EventHotKeyRef,
    ) -> OsStatus,
    unregister_event_hot_key: unsafe extern "C" fn(EventHotKeyRef) -> OsStatus,
    get_event_parameter: unsafe extern "C" fn(
        EventRef,
        u32,
        u32,
        *mut u32,
        c_ulong,
        *mut c_ulong,
        *mut c_void,
    ) -> OsStatus,
}

fn carbon() -> Option<&'static Carbon> {
    static CARBON: OnceLock<Option<Carbon>> = OnceLock::new();
    CARBON
        .get_or_init(|| unsafe {
            // HIToolbox directly; the Carbon umbrella re-exports it and is
            // the fallback should the sub-framework path ever move.
            let lib = libloading::Library::new(
                "/System/Library/Frameworks/Carbon.framework/Frameworks/HIToolbox.framework/HIToolbox",
            )
            .or_else(|_| {
                libloading::Library::new("/System/Library/Frameworks/Carbon.framework/Carbon")
            })
            .ok()?;
            let get_application_event_target = *lib
                .get::<unsafe extern "C" fn() -> EventTargetRef>(b"GetApplicationEventTarget\0")
                .ok()?;
            let install_event_handler = *lib
                .get::<unsafe extern "C" fn(
                    EventTargetRef,
                    EventHandlerProc,
                    c_ulong,
                    *const EventTypeSpec,
                    *mut c_void,
                    *mut EventHandlerRef,
                ) -> OsStatus>(b"InstallEventHandler\0")
                .ok()?;
            let register_event_hot_key = *lib
                .get::<unsafe extern "C" fn(
                    u32,
                    u32,
                    EventHotKeyId,
                    EventTargetRef,
                    u32,
                    *mut EventHotKeyRef,
                ) -> OsStatus>(b"RegisterEventHotKey\0")
                .ok()?;
            let unregister_event_hot_key = *lib
                .get::<unsafe extern "C" fn(EventHotKeyRef) -> OsStatus>(
                    b"UnregisterEventHotKey\0",
                )
                .ok()?;
            let get_event_parameter = *lib
                .get::<unsafe extern "C" fn(
                    EventRef,
                    u32,
                    u32,
                    *mut u32,
                    c_ulong,
                    *mut c_ulong,
                    *mut c_void,
                ) -> OsStatus>(b"GetEventParameter\0")
                .ok()?;
            std::mem::forget(lib);
            Some(Carbon {
                get_application_event_target,
                install_event_handler,
                register_event_hot_key,
                unregister_event_hot_key,
                get_event_parameter,
            })
        })
        .as_ref()
}

/// The live `EventHotKeyRef`s, by registry id. Stored as `usize` so the map
/// is `Send`; they are only ever touched on the main thread.
static HOT_KEYS: Mutex<BTreeMap<u32, usize>> = Mutex::new(BTreeMap::new());

/// The Carbon handler, called on the main thread inside AppKit's dispatch.
/// Parks the id and returns; never unwinds (a poisoned lock is recovered by
/// `push_fired`).
unsafe extern "C" fn hot_key_handler(
    _call: EventHandlerCallRef,
    event: EventRef,
    _user_data: *mut c_void,
) -> OsStatus {
    let Some(carbon) = carbon() else {
        return EVENT_NOT_HANDLED_ERR;
    };
    let mut hot_key = EventHotKeyId {
        signature: 0,
        id: 0,
    };
    let status = unsafe {
        (carbon.get_event_parameter)(
            event,
            K_EVENT_PARAM_DIRECT_OBJECT,
            TYPE_EVENT_HOT_KEY_ID,
            core::ptr::null_mut(),
            core::mem::size_of::<EventHotKeyId>() as c_ulong,
            core::ptr::null_mut(),
            (&mut hot_key as *mut EventHotKeyId).cast::<c_void>(),
        )
    };
    if status != NO_ERR || hot_key.signature != SIGNATURE {
        return EVENT_NOT_HANDLED_ERR;
    }
    push_fired(GlobalHotkeyId { id: hot_key.id });
    NO_ERR
}

/// Install the handler on the application event target, once. `Err` when
/// Carbon refused - then no hot key could ever be delivered.
fn ensure_handler(carbon: &Carbon) -> Result<(), GlobalHotkeyError> {
    static INSTALLED: OnceLock<Result<(), i32>> = OnceLock::new();
    let installed = INSTALLED.get_or_init(|| {
        let spec = EventTypeSpec {
            event_class: K_EVENT_CLASS_KEYBOARD,
            event_kind: K_EVENT_HOT_KEY_PRESSED,
        };
        let mut handler_ref: EventHandlerRef = core::ptr::null_mut();
        let status = unsafe {
            (carbon.install_event_handler)(
                (carbon.get_application_event_target)(),
                hot_key_handler,
                1,
                &spec,
                core::ptr::null_mut(),
                &mut handler_ref,
            )
        };
        if status == NO_ERR {
            Ok(())
        } else {
            Err(status)
        }
    });
    match installed {
        Ok(()) => Ok(()),
        Err(status) => Err(GlobalHotkeyError::Platform(
            format!("InstallEventHandler failed (OSStatus {status})").into(),
        )),
    }
}

/// The macOS virtual keycode (`kVK_*`) for `key`: the inverse of the table
/// the window's own `keyDown:` uses, so a hot key and an in-window shortcut
/// name the same physical key. Positional (ANSI layout), like that table.
fn keycode_of(key: azul_core::window::VirtualKeyCode) -> Option<u32> {
    (0u16..0x80)
        .find(|code| {
            crate::desktop::shell2::common::event::macos_keycode_to_virtual_key(*code) == Some(key)
        })
        .map(u32::from)
}

fn carbon_modifiers(hotkey: &GlobalHotkey) -> u32 {
    let m = hotkey.modifiers;
    let mut bits = 0;
    if m.meta {
        bits |= CMD_KEY;
    }
    if m.shift {
        bits |= SHIFT_KEY;
    }
    if m.alt {
        bits |= OPTION_KEY;
    }
    if m.ctrl {
        bits |= CONTROL_KEY;
    }
    bits
}

fn probe() -> Result<(), String> {
    if carbon().is_some() {
        Ok(())
    } else {
        Err(String::from(
            "HIToolbox (Carbon.framework) could not be loaded, so RegisterEventHotKey is missing",
        ))
    }
}

fn register(id: GlobalHotkeyId, hotkey: &GlobalHotkey) -> Result<BackendGrant, GlobalHotkeyError> {
    let Some(carbon) = carbon() else {
        return Err(GlobalHotkeyError::Unavailable(
            "HIToolbox (Carbon.framework) could not be loaded".into(),
        ));
    };
    ensure_handler(carbon)?;
    let Some(keycode) = keycode_of(hotkey.key) else {
        return Err(GlobalHotkeyError::KeyNotMappable);
    };
    let mut hot_key_ref: EventHotKeyRef = core::ptr::null_mut();
    let status = unsafe {
        (carbon.register_event_hot_key)(
            keycode,
            carbon_modifiers(hotkey),
            EventHotKeyId {
                signature: SIGNATURE,
                id: id.id,
            },
            (carbon.get_application_event_target)(),
            K_EVENT_HOT_KEY_EXCLUSIVE,
            &mut hot_key_ref,
        )
    };
    match status {
        NO_ERR if !hot_key_ref.is_null() => {
            HOT_KEYS
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(id.id, hot_key_ref as usize);
            Ok(BackendGrant::Active)
        }
        EVENT_HOT_KEY_EXISTS_ERR => Err(GlobalHotkeyError::TakenByAnotherApp),
        EVENT_HOT_KEY_INVALID_ERR => Err(GlobalHotkeyError::KeyNotMappable),
        other => Err(GlobalHotkeyError::Platform(
            format!("RegisterEventHotKey failed (OSStatus {other})").into(),
        )),
    }
}

fn unregister(id: GlobalHotkeyId) {
    let hot_key_ref = HOT_KEYS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&id.id);
    if let (Some(carbon), Some(hot_key_ref)) = (carbon(), hot_key_ref) {
        unsafe {
            let _ = (carbon.unregister_event_hot_key)(hot_key_ref as EventHotKeyRef);
        }
    }
}

/// Nothing to poll: AppKit's dispatch runs the handler.
fn poll() {}

pub(super) fn backend() -> GlobalHotkeyBackend {
    GlobalHotkeyBackend {
        name: "Carbon RegisterEventHotKey (HIToolbox)",
        probe,
        register,
        unregister,
        poll,
        // The hot key is an NSEvent: it wakes the run loop by itself.
        needs_loop_polling: false,
    }
}
