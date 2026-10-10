//! macOS and iOS: Network.framework's path monitor (its C API, `nw_path_monitor_*`), loaded at
//! runtime like the other frameworks of `extra` (MediaPlayer, IOKit): a system without it
//! (before macOS 10.14 / iOS 12) has no reading. The monitor runs on a serial dispatch queue of
//! its own; its update handler (a block) reads each new path there and keeps it for the next
//! query:
//!
//! - connected: `nw_path_get_status` is satisfied;
//! - metered: `nw_path_is_expensive` (a cellular path, a phone's Personal Hotspot);
//! - constrained: `nw_path_is_constrained` (Low Data Mode; macOS 10.15 / iOS 13, false before);
//! - hotspot: an expensive Wi-Fi path (a phone's Personal Hotspot; [`super::hotspot_guess`]);
//! - the kind: `nw_path_uses_interface_type` - Wi-Fi, cellular, wired, else other (a VPN's
//!   path uses its own interface over the physical one, which is found first).

use std::{
    ffi::{c_char, c_void},
    sync::OnceLock,
};

use block2::RcBlock;

use super::{hotspot_guess, last_seen, seen, NetworkKind, NetworkState};

const NETWORK_FRAMEWORK: &str = "/System/Library/Frameworks/Network.framework/Network";

/// `nw_path_status_satisfied` (Network/path.h).
const NW_PATH_STATUS_SATISFIED: i32 = 1;
/// `nw_interface_type_t` (Network/interface.h).
const NW_INTERFACE_TYPE_WIFI: i32 = 1;
const NW_INTERFACE_TYPE_CELLULAR: i32 = 2;
const NW_INTERFACE_TYPE_WIRED: i32 = 3;

type MonitorCreateFn = unsafe extern "C" fn() -> *mut c_void;
type MonitorSetQueueFn = unsafe extern "C" fn(*mut c_void, *mut c_void);
type MonitorSetUpdateHandlerFn = unsafe extern "C" fn(*mut c_void, *mut c_void);
type MonitorStartFn = unsafe extern "C" fn(*mut c_void);
type PathGetStatusFn = unsafe extern "C" fn(*mut c_void) -> i32;
type PathFlagFn = unsafe extern "C" fn(*mut c_void) -> bool;
type PathUsesInterfaceTypeFn = unsafe extern "C" fn(*mut c_void, i32) -> bool;

extern "C" {
    /// libdispatch, part of libSystem (which every process links). A null attribute is a
    /// serial queue.
    fn dispatch_queue_create(label: *const c_char, attr: *mut c_void) -> *mut c_void;
}

/// What the update handler reads a path with.
#[derive(Clone, Copy)]
struct PathFns {
    status: PathGetStatusFn,
    expensive: PathFlagFn,
    /// Missing before macOS 10.15 / iOS 13.
    constrained: Option<PathFlagFn>,
    uses_interface_type: PathUsesInterfaceTypeFn,
}

/// The reading of `path`.
///
/// # Safety
///
/// `path` is the path the monitor handed its update handler (valid while the handler runs), or
/// null.
unsafe fn reading_of(path: *mut c_void, f: &PathFns) -> NetworkState {
    // SAFETY: the caller's - `path` is a live path or null (checked first).
    if path.is_null() || unsafe { (f.status)(path) } != NW_PATH_STATUS_SATISFIED {
        return NetworkState::OFFLINE;
    }
    // SAFETY: as above.
    let uses = |kind: i32| unsafe { (f.uses_interface_type)(path, kind) };
    let kind = if uses(NW_INTERFACE_TYPE_WIFI) {
        NetworkKind::WiFi
    } else if uses(NW_INTERFACE_TYPE_CELLULAR) {
        NetworkKind::Cellular
    } else if uses(NW_INTERFACE_TYPE_WIRED) {
        NetworkKind::Wired
    } else {
        NetworkKind::Other
    };
    // SAFETY: as above.
    let (expensive, constrained) = unsafe {
        (
            (f.expensive)(path),
            f.constrained.is_some_and(|constrained| constrained(path)),
        )
    };
    NetworkState {
        kind,
        connected: true,
        metered: expensive,
        constrained,
        hotspot: hotspot_guess(kind, expensive),
    }
}

/// Starts the monitor; `None` where Network.framework (or a function of it) is missing.
///
/// # Safety
///
/// Called once (`started`): the symbols have exactly these signatures (Network/path.h,
/// Network/path_monitor.h).
unsafe fn start() -> Option<()> {
    // SAFETY: the caller's (above).
    unsafe {
        let lib = libloading::Library::new(NETWORK_FRAMEWORK).ok()?;
        let create = *lib
            .get::<MonitorCreateFn>(b"nw_path_monitor_create\0")
            .ok()?;
        let set_queue = *lib
            .get::<MonitorSetQueueFn>(b"nw_path_monitor_set_queue\0")
            .ok()?;
        let set_update_handler = *lib
            .get::<MonitorSetUpdateHandlerFn>(b"nw_path_monitor_set_update_handler\0")
            .ok()?;
        let start_monitor = *lib.get::<MonitorStartFn>(b"nw_path_monitor_start\0").ok()?;
        let fns = PathFns {
            status: *lib.get::<PathGetStatusFn>(b"nw_path_get_status\0").ok()?,
            expensive: *lib.get::<PathFlagFn>(b"nw_path_is_expensive\0").ok()?,
            constrained: lib
                .get::<PathFlagFn>(b"nw_path_is_constrained\0")
                .ok()
                .map(|symbol| *symbol),
            uses_interface_type: *lib
                .get::<PathUsesInterfaceTypeFn>(b"nw_path_uses_interface_type\0")
                .ok()?,
        };
        let monitor = create();
        if monitor.is_null() {
            return None;
        }
        let queue =
            dispatch_queue_create(c"org.azul.network-monitor".as_ptr(), std::ptr::null_mut());
        if queue.is_null() {
            return None;
        }
        // SAFETY (of the block's body): the monitor calls it on its queue with the new path.
        let handler =
            RcBlock::new(move |path: *mut c_void| seen(unsafe { reading_of(path, &fns) }));
        set_queue(monitor, queue);
        set_update_handler(monitor, RcBlock::as_ptr(&handler).cast());
        start_monitor(monitor);
        // The monitor copied the block; it, its queue and the framework stay for the process's
        // life, so nothing of them is released.
        std::mem::forget(handler);
        std::mem::forget(lib);
        Some(())
    }
}

/// Whether the monitor runs (started by the first call).
fn started() -> bool {
    static STARTED: OnceLock<bool> = OnceLock::new();
    // SAFETY: once, through the OnceLock.
    *STARTED.get_or_init(|| unsafe { start() }.is_some())
}

/// The monitor's last reading; `None` before its first one and without Network.framework.
pub(super) fn read() -> Option<NetworkState> {
    if !started() {
        return None;
    }
    last_seen()
}
