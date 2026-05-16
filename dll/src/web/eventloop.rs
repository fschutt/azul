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
    /// Callback registry: `pack_cb_key(node_idx, event_kind)` → table
    /// index in the JS-owned `WebAssembly.Table`. JS populates this
    /// at bootstrap via [`AzStartup_registerCallback`] as each
    /// per-callback WASM finishes instantiating.
    pub cb_table_indices: BTreeMap<u64, u32>,
    /// Pending TLV-encoded DOM patch bytes — drained by
    /// [`AzStartup_getPatches`].
    pub pending_patches: Vec<u8>,
}

/// Pack a `(node_idx, event_kind)` pair into a u64 BTreeMap key. The
/// node_idx occupies the high 32 bits so a `range` over a single
/// node's bindings can use `(node_idx<<32)..((node_idx+1)<<32)`.
#[inline]
fn pack_cb_key(node_idx: u32, event_kind: u32) -> u64 {
    ((node_idx as u64) << 32) | (event_kind as u64)
}

// =====================================================================
// Event-format spec (Q5 decision: fixed 256-byte buffer per dispatch).
// JS-side packing must match. See M8.6 listener.js for the encoder.
// =====================================================================

/// Fixed event-buffer size. 256 bytes leaves headroom for IME
/// composition strings + future touch events with multiple contact
/// points beyond hello-world's mouse/keyboard needs. JS allocates
/// this size via [`AzStartup_alloc`] before every dispatch and frees
/// after.
pub const EVENT_BYTES_LEN: u32 = 256;

/// Event-kind discriminator passed as `AzStartup_dispatchEvent`'s
/// first arg. Indices match azul's existing EventFilter ordering for
/// the cases that map directly; the rest are sequential.
pub mod event_kind {
    pub const CLICK:      u32 = 0;
    pub const MOUSEDOWN:  u32 = 1;
    pub const MOUSEUP:    u32 = 2;
    pub const MOUSEMOVE:  u32 = 3;
    pub const DBLCLICK:   u32 = 4;
    pub const WHEEL:      u32 = 5;
    pub const KEYDOWN:    u32 = 6;
    pub const KEYUP:      u32 = 7;
    pub const FOCUSIN:    u32 = 8;
    pub const FOCUSOUT:   u32 = 9;
    pub const RESIZE:     u32 = 10;
    pub const SCROLL:     u32 = 11;
}

/// Common event-bytes layout offsets. JS writes these fields with
/// `DataView.setUint32(off, val, /*littleEndian=*/ true)`. Per-kind
/// extras (e.g. mouse `button`, keyboard `key_code`) extend past
/// `MODIFIERS`.
pub mod event_offset {
    /// `u32` — synthetic `az_N` node ID under the event target. JS
    /// derives this from `event.target.id.match(/^az_(\d+)$/)`.
    /// `0xFFFFFFFF` means "no node found" (window-level event).
    pub const NODE_IDX:  u32 = 0;
    /// `f32` — clientX in CSS pixels. 0 for non-pointer events.
    pub const X:         u32 = 4;
    /// `f32` — clientY in CSS pixels.
    pub const Y:         u32 = 8;
    /// `u32` — `event.button` for mouse / `event.keyCode` for keys.
    pub const BUTTON_OR_KEY: u32 = 12;
    /// `u32` — modifier-key bitmap: bit0=shift bit1=ctrl bit2=alt bit3=meta.
    pub const MODIFIERS: u32 = 16;
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
// Callback registration
// =====================================================================

/// Register a `(node_idx, event_kind) → table_idx` binding so
/// [`AzStartup_dispatchEvent`] can route events to the right
/// per-callback WASM. JS calls this once per discovered
/// `[data-az-cb][data-az-ev]` element after instantiating that
/// element's callback module.
///
/// Returns `0` on success, `1` if [`EVENTLOOP_PTR`] is null (caller
/// forgot [`AzStartup_init`]). Subsequent registrations for the same
/// `(node_idx, event_kind)` overwrite the previous `table_idx` —
/// matches the "last write wins" semantics of `re_render_body`.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_registerCallback(
    node_idx: u32,
    event_kind: u32,
    table_idx: u32,
) -> u32 {
    let p = EVENTLOOP_PTR.load(Ordering::SeqCst);
    if p.is_null() {
        return 1;
    }
    let state = &mut *p;
    state.cb_table_indices.insert(pack_cb_key(node_idx, event_kind), table_idx);
    0
}

// =====================================================================
// Event dispatch
// =====================================================================

/// Process one input event from JS.
///
/// JS marshals the native DOM event into a fixed
/// [`EVENT_BYTES_LEN`]-byte buffer; `kind` selects the
/// [`event_kind`] variant. Returns the number of patches queued
/// (caller drains via [`AzStartup_getPatches`]).
///
/// **M8.4a — lookup-only stage**: this body extracts `node_idx` from
/// the event buffer + looks up the registered callback in
/// [`EventloopState::cb_table_indices`]. The actual `call_indirect`
/// dispatch into the JS-owned `WebAssembly.Table` lands in M8.4b
/// (requires a thin `__az_call_indirect` helper IR in the linked
/// azul-mini.wasm to bridge from Rust source through to a wasm-side
/// `call_indirect`). For now the return value reports whether a
/// matching binding was found (`1`) or not (`0`); JS treats both as
/// "no patches queued" until M8.4b lands.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_dispatchEvent(
    kind: u32,
    event_bytes_ptr: u32,
    event_bytes_len: u32,
) -> u32 {
    if event_bytes_len < event_offset::MODIFIERS + 4 {
        return 0;
    }
    let p = EVENTLOOP_PTR.load(Ordering::SeqCst);
    if p.is_null() {
        return 0;
    }
    let state = &mut *p;
    // Read node_idx (u32 LE) from the JS-marshalled buffer. The
    // pointer is a linear-memory offset; on wasm32 the cast becomes
    // a no-op, on native AArch64 (only the lift-source path) it
    // truncates harmlessly because the body never executes natively.
    let node_idx_ptr = (event_bytes_ptr as usize + event_offset::NODE_IDX as usize) as *const u32;
    let node_idx = core::ptr::read_unaligned(node_idx_ptr);
    if node_idx == u32::MAX {
        // Window-level event without a target node (resize, focus
        // chrome). M8.4c will route to window-callbacks; for M8.4a
        // we just drop it.
        return 0;
    }
    let key = pack_cb_key(node_idx, kind);
    match state.cb_table_indices.get(&key) {
        Some(_table_idx) => 1, // M8.4b: call_indirect here.
        None => 0,
    }
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
