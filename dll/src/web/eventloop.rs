//! Eventloop / HeadlessWindow-simulator surface.
//!
//! Defines the `AzStartup_*` C-ABI functions that get lifted via the
//! M5-M7 remill pipeline into `azul-mini.wasm` at server startup. The
//! lifted module is what JS calls to drive the browser-side event
//! loop. See `scripts/M8_ARCHITECTURE_2026_05_19.md`.
//!
//! M8.1 ships the source surface only — bodies are functional stubs
//! sufficient to compile, export symbols, and let the lift pipeline
//! see bytes. Real implementations land in M8.4 (dispatch), M8.5
//! (patches), M8.7 (init).
//!
//! Pointer types are `u32` because in the lifted WASM, addresses are
//! 32-bit linear-memory offsets. On native AArch64 the cast
//! `usize as u32` truncates the high bits — that's harmless because
//! these functions are only invoked from JS through the lifted module,
//! never natively. The native build exists so that `dlsym` + the
//! remill pipeline have function bytes to read at server startup.

use core::ptr::null_mut;
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use std::alloc::{alloc, dealloc, Layout};
use std::collections::BTreeMap;

use azul_core::refany::RefAny;
use azul_core::styled_dom::StyledDom;

/// Single-tab/single-window: one global EventloopState pointer.
/// Null until [`AzStartup_init`] populates it.
static EVENTLOOP_PTR: AtomicPtr<EventloopState> = AtomicPtr::new(null_mut());

/// User-supplied `<Type>_fromJson` fn-pointer registered via
/// [`AzStartup_registerStateDeserializer`]. Zero means none
/// registered, in which case [`AzStartup_init`] falls back to the
/// raw-RefAny-bytes path (riskier; user-acknowledged).
static STATE_DESERIALIZER: AtomicUsize = AtomicUsize::new(0);

/// Browser-side eventloop state. One per tab.
pub struct EventloopState {
    /// User's app data, materialised from the initial JSON. `None`
    /// until [`AzStartup_init`] hydrates it (M8.7).
    pub app_data: Option<RefAny>,
    /// Most recent layout output. Hit-tested against on incoming
    /// events; reconciled against on RefreshDom. `None` until the
    /// first layout callback run.
    pub current_dom: Option<StyledDom>,
    /// Callback registry: `node_idx` → table index in the JS-owned
    /// `WebAssembly.Table`. Populated as per-callback WASMs load.
    pub cb_table_indices: BTreeMap<u32, u32>,
    /// Pending TLV-encoded DOM patch bytes — drained by
    /// [`AzStartup_getPatches`].
    pub pending_patches: Vec<u8>,
}

// =====================================================================
// Allocator surface — shared across all lifted modules (Q3 decision:
// shared in azul-mini). Layout + per-callback WASMs import these.
// =====================================================================

/// Allocate `size` bytes of zero-initialised storage and return the
/// linear-memory offset. Returns 0 on failure.
///
/// JS uses this to stage:
///   - the initial state JSON before calling [`AzStartup_init`],
///   - per-event 256-byte buffers before [`AzStartup_dispatchEvent`],
///   - readback buffers for [`AzStartup_getPatches`].
#[no_mangle]
pub extern "C" fn AzStartup_alloc(size: u32) -> u32 {
    if size == 0 {
        return 0;
    }
    let Ok(layout) = Layout::from_size_align(size as usize, 8) else {
        return 0;
    };
    let ptr = unsafe { alloc(layout) };
    ptr as usize as u32
}

/// Free a buffer previously returned by [`AzStartup_alloc`]. `size`
/// must match the original alloc size (we use `Layout` rather than
/// requesting a `realloc`-style sized free from the underlying
/// allocator).
#[no_mangle]
pub extern "C" fn AzStartup_free(ptr: u32, size: u32) {
    if ptr == 0 || size == 0 {
        return;
    }
    let Ok(layout) = Layout::from_size_align(size as usize, 8) else {
        return;
    };
    unsafe { dealloc(ptr as usize as *mut u8, layout) };
}

// =====================================================================
// Lifecycle
// =====================================================================

/// Hydrate the global state from the server-embedded payload.
///
/// `json_ptr` + `json_len` describe a byte buffer in shared linear
/// memory. Behaviour (M8.7 spec):
///   - If [`STATE_DESERIALIZER`] is registered, call it with the JSON
///     bytes to obtain the initial `RefAny`.
///   - Otherwise attempt to decode the bytes as a raw `RefAny`
///     serialization (riskier — requires the user binding to opt in).
///
/// Returns 0 on success, non-zero on failure.
///
/// **M8.1 stub**: allocates an empty [`EventloopState`], swaps it
/// into [`EVENTLOOP_PTR`], returns 0. The payload is ignored until
/// M8.7.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_init(_json_ptr: u32, _json_len: u32) -> u32 {
    let state = Box::new(EventloopState {
        app_data: None,
        current_dom: None,
        cb_table_indices: BTreeMap::new(),
        pending_patches: Vec::new(),
    });
    let raw = Box::into_raw(state);
    let prev = EVENTLOOP_PTR.swap(raw, Ordering::SeqCst);
    if !prev.is_null() {
        // Re-init: free the previous state. Page refresh path.
        drop(Box::from_raw(prev));
    }
    0
}

/// Register the user-supplied `<Type>_fromJson` fn-pointer that
/// [`AzStartup_init`] consults during hydration.
///
/// Called by the framework (under `AZ_BACKEND=web://`) from the
/// expansion of the `AZ_REFLECT_JSON` C macro. The fn-pointer is
/// opaque here for M8.1; the typed signature
/// (`extern "C" fn(AzJson) -> AzResultRefAnyString`) is finalised in
/// M8.7 once the codegen types are wired through.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_registerStateDeserializer(fn_ptr: usize) {
    STATE_DESERIALIZER.store(fn_ptr, Ordering::SeqCst);
}

// =====================================================================
// Event dispatch
// =====================================================================

/// Process one input event from JS.
///
/// JS marshals the native DOM event into a fixed 256-byte buffer
/// (event-format spec in M8.6). `kind` selects the union variant
/// (mouse / keyboard / wheel / focus / resize). Returns the count of
/// patches queued (0 means no DOM mutation needed).
///
/// **M8.1 stub**: returns 0. M8.4 wires hit-test + EventFilter
/// dispatch + indirect callback invocation.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_dispatchEvent(
    _kind: u32,
    _event_bytes_ptr: u32,
    _event_bytes_len: u32,
) -> u32 {
    0
}

/// Drain queued DOM mutations into the JS-allocated readback buffer
/// as a TLV byte stream:
///   `kind:u8 | node_idx:u32 | payload_len:u32 | payload:[u8; payload_len]`
/// Returns bytes actually written (0 if no pending patches, or if the
/// buffer is too small — caller should retry with a larger buffer).
///
/// **M8.1 stub**: returns 0. M8.5 wires real patch emission.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_getPatches(_out_ptr: u32, _out_cap: u32) -> u32 {
    0
}
