//! Eventloop / HeadlessWindow-simulator surface.
//!
//! Defines the `AzStartup_*` C-ABI functions that get lifted via the
//! M5-M7 remill pipeline into `azul-mini.wasm` at server startup.
//! The lifted module is what JS calls to drive the browser-side
//! event loop. See `scripts/M8_ARCHITECTURE_2026_05_19.md` and the
//! M8.4b reset captured in `memory/m8_4_lift_runtime_gap_2026_05_16.md`.
//!
//! # Model (M8.4b reset, per user correction 2026-05-16)
//!
//! `AzStartup_init` creates the global App in WASM (RefAny +
//! current StyledDom + layout-callback fn-ptr). Native equivalent:
//! `AzApp::new(app_data, app_config)`. Returns the App pointer.
//!
//! `AzStartup_dispatchEvent(state, kind, evt_bytes, evt_len,
//! out_len_ptr) -> patches_ptr` does *everything* per event,
//! synchronously:
//!   1. Decodes the event bytes.
//!   2. Hit-tests in WASM against the App's StyledDom.
//!   3. Identifies the user-callback fn-ptr stored on the matching
//!      node.
//!   4. Calls back to JS via the imported `__az_resolve_callback`
//!      to translate that fn-ptr to a `WebAssembly.Table` index.
//!   5. `call_indirect(idx, refany_lo, refany_hi, info_ptr)` →
//!      gets the user's `Update` result.
//!   6. If `Update::RefreshDom`: invokes layout callback + diffs
//!      against the old StyledDom + emits TLV patches.
//!   7. Writes the patch byte-stream length to `*out_len_ptr` and
//!      returns the patch buffer's wasm address.
//!
//! JS owns the addr→table mapping (it pre-instantiated all per-cb
//! WASMs at bootstrap and put each in its table slot). WASM owns
//! the App state + the dispatch+diff logic.
//!
//! # Why no statics
//!
//! Rust source-level statics generate AArch64 `adrp+add+ldr` for
//! reads, which remill lifts to address arithmetic based on
//! lift_addr — those wasm offsets don't correspond to wasm-ld's
//! data-section placement of the static. Heap-allocated state
//! sidesteps this: addresses flow from `__rust_alloc`'s return
//! value, and as long as that's a valid wasm32 offset (M8.4c
//! provides a bump-allocator body in helper IR), every subsequent
//! deref stays valid.
//!
//! That's why `AzStartup_init` returns the state pointer instead of
//! storing it in a `static EVENTLOOP_PTR`. JS threads the pointer
//! back through every subsequent call.

use std::alloc::{alloc, dealloc, Layout};
use std::collections::BTreeMap;

use azul_core::refany::RefAny;
use azul_core::styled_dom::StyledDom;

// =====================================================================
// Event-format spec (Q5 decision: fixed 256-byte buffer per dispatch).
// JS-side packing must match. See M8.6 listener.js for the encoder.
// =====================================================================

/// Fixed event-buffer size. 256 bytes leaves headroom for IME
/// composition + future touch events beyond hello-world's mouse/key
/// needs.
pub const EVENT_BYTES_LEN: u32 = 256;

/// Event-kind discriminator passed as the first non-state arg to
/// `AzStartup_dispatchEvent`. Indices match azul's existing
/// EventFilter ordering for the cases that map directly.
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

/// Common event-bytes layout offsets. JS writes these with
/// `DataView.setUint32/Float32(off, val, /*LE=*/ true)`. Per-kind
/// extras live past `MODIFIERS`.
pub mod event_offset {
    pub const NODE_IDX:      u32 = 0;
    pub const X:             u32 = 4;
    pub const Y:             u32 = 8;
    pub const BUTTON_OR_KEY: u32 = 12;
    pub const MODIFIERS:     u32 = 16;
}

/// Browser-side App state. One per page. Returned from
/// [`AzStartup_init`] as a heap pointer that JS threads back through
/// every subsequent call.
pub struct EventloopState {
    /// User's app data, materialised by the user-registered JSON
    /// deserializer during [`AzStartup_init`]. `None` if no
    /// deserializer was registered + the raw-RefAny-bytes fallback
    /// also failed.
    pub app_data: Option<RefAny>,
    /// Most recent layout output. Hit-tested against incoming events;
    /// reconciled against on RefreshDom. `None` until the first
    /// layout-callback run inside dispatch.
    pub current_dom: Option<StyledDom>,
    /// User-supplied `<Type>_fromJson` fn-pointer set via
    /// [`AzStartup_registerStateDeserializer`]. Zero = unset.
    pub state_deserializer: u64,
    /// TLV-encoded DOM patch bytes — written by dispatch, read by
    /// the caller via the return values of dispatchEvent.
    pub patches: Vec<u8>,
    /// Bookkeeping for callback dispatch: cached node→callback-fn-ptr
    /// associations harvested from the StyledDom on first
    /// hit-test. Populated lazily; cleared on RefreshDom.
    pub cb_fn_cache: BTreeMap<u32, u64>,
}

// =====================================================================
// Imports satisfied by JS at instantiation time (see M8.6 listener.js).
// =====================================================================

/// Translate a native callback fn-pointer address (as stored on a
/// StyledDom node) to the `WebAssembly.Table` index that holds the
/// per-callback WASM's `callback` export. JS owns the addr→table
/// mapping: at bootstrap it instantiates every
/// `/az/cb/<sym>.<hash>.wasm` and records each module's "real"
/// fn-addr (from a side-channel — typically the `data-az-cb-addr`
/// attribute the server emits per node) under the table slot it
/// placed the module's `callback` export in.
///
/// Returns `0xFFFFFFFF` if the addr isn't registered.
///
/// Native body is a stub for linker satisfaction (never reached
/// natively). At lift time the M8.5a intercept replaces the body
/// with a wasm-imported call to `env.__az_resolve_callback`
/// satisfied by the loader.js bootstrap.
///
/// Both args go through `core::hint::black_box` so the optimizer
/// can't perform dead-arg elimination at native call sites. Without
/// this, the native compiler sees `_cb_fn_addr` as unused → doesn't
/// bother setting up X0 with the real address at the call site →
/// the lifted body's `bl` site has stale/zero X0 → JS-side import
/// receives 0n instead of the actual address.
#[no_mangle]
#[inline(never)]
pub unsafe extern "C" fn __az_resolve_callback(cb_fn_addr: u64) -> u32 {
    let _ = core::hint::black_box(cb_fn_addr);
    core::hint::black_box(u32::MAX)
}

/// Wasm `call_indirect` bridge.
///
/// Calls `WebAssembly.Table[table_idx]` with signature
/// `(i64, i64, i32) -> i32`. The four-arg shape matches every
/// per-callback wrapper produced by the M5-M7 pipeline — they all
/// expose `callback(refany_lo: i64, refany_hi: i64, info_ptr: i32)
/// -> i32` regardless of which user-callback typedef they came from.
///
/// Native body is a no-op stub for linker satisfaction (this is
/// never called natively — the lift's `bl ___az_call_indirect` site
/// gets replaced by [`transpiler_remill::emit_helper_ir`] with a
/// wasm-side `call_indirect` via `inttoptr i32 %tidx to ptr` + a
/// typed `call`).
#[no_mangle]
#[inline(never)]
pub unsafe extern "C" fn __az_call_indirect(
    table_idx: u32,
    refany_lo: u64,
    refany_hi: u64,
    info_ptr: u32,
) -> u32 {
    // Never reached natively. The lift-time intercept replaces the
    // body with a wasm call_indirect. All args go through black_box
    // to defeat the native compiler's dead-arg elimination (same
    // pattern as __az_resolve_callback).
    let _ = core::hint::black_box(table_idx);
    let _ = core::hint::black_box(refany_lo);
    let _ = core::hint::black_box(refany_hi);
    let _ = core::hint::black_box(info_ptr);
    core::hint::black_box(0)
}

// =====================================================================
// Allocator surface — bump allocator backed by __rust_alloc, which
// M8.4c provides as a hand-written bump-impl body in helper IR.
// Layout + per-callback WASMs import these.
// =====================================================================

/// Allocate `size` bytes of zero-initialised storage and return the
/// linear-memory offset. Returns 0 on failure.
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

/// Free a buffer previously returned by [`AzStartup_alloc`].
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

/// Allocate the App and return its pointer.
///
/// `json_ptr` + `json_len` describe the server-embedded initial
/// state payload (see `<script id="az-state">` in the rendered
/// HTML). If a JSON deserializer has been registered via
/// [`AzStartup_registerStateDeserializer`] before this call, it's
/// invoked to produce the initial `RefAny`; otherwise the raw-bytes
/// fallback path is attempted (M8.7).
///
/// Returns the App pointer (as a u32 wasm linear-memory offset),
/// or `0` on allocation failure.
///
/// **M8.4b stub**: Box-allocates an empty `EventloopState` and
/// returns its pointer. The JSON payload is ignored until M8.7; the
/// deserializer-vs-raw fallback choice is also M8.7. M8.4c provides
/// the `__rust_alloc` bump-allocator body so `Box::new` actually
/// returns a valid wasm pointer (today it traps because the lift's
/// `__rust_alloc` call is noop-stubbed).
#[no_mangle]
pub unsafe extern "C" fn AzStartup_init(_json_ptr: u32, _json_len: u32) -> u32 {
    let state = Box::new(EventloopState {
        app_data: None,
        current_dom: None,
        state_deserializer: 0,
        patches: Vec::new(),
        cb_fn_cache: BTreeMap::new(),
    });
    Box::into_raw(state) as usize as u32
}

/// Record the user-supplied `<Type>_fromJson` fn-pointer on the App
/// so [`AzStartup_init`]'s deserialization step can call it.
///
/// Should be called BEFORE `AzStartup_init` for the deserializer to
/// take effect; calling after init is allowed but won't retroactively
/// re-deserialize.
///
/// `state` is the App pointer returned by `AzStartup_init` (or 0 if
/// being called before init — in which case the call is a no-op
/// and the deserializer setting is lost).
#[no_mangle]
pub unsafe extern "C" fn AzStartup_registerStateDeserializer(
    state: u32,
    fn_addr: u64,
) {
    if state == 0 {
        return;
    }
    let s = &mut *(state as usize as *mut EventloopState);
    s.state_deserializer = fn_addr;
}

// =====================================================================
// Event dispatch
// =====================================================================

/// Process one input event, returning the patch byte-stream.
///
/// Writes the patch-buffer length (in bytes) to `*out_len_ptr`.
/// Returns the patch buffer's wasm linear-memory offset (`0` if no
/// patches were produced).
///
/// **M8.5a partial dispatch**: extracts `node_idx` from event_bytes,
/// looks up the App's `cb_fn_cache` for the cb fn-addr at that
/// node, resolves to a table index via [`__az_resolve_callback`],
/// invokes via [`__az_call_indirect`]. Patches aren't emitted yet
/// (M8.5b adds the diff loop); the return value reports the
/// Update enum the user callback produced as a debugging signal.
///
/// For M8.5a there's no real hit-test or cb-fn-cache population —
/// `node_idx` IS treated as the cb fn-addr lookup key (test
/// fixture). M8.5b populates the cache from the StyledDom.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_dispatchEvent(
    state: u32,
    _kind: u32,
    event_bytes_ptr: u32,
    event_bytes_len: u32,
    out_len_ptr: u32,
) -> u32 {
    if state == 0 || out_len_ptr == 0 {
        return 0;
    }
    if event_bytes_len < event_offset::MODIFIERS + 4 {
        core::ptr::write_unaligned(out_len_ptr as usize as *mut u32, 0);
        return 0;
    }
    // Read node_idx from event buffer. Use `read` (aligned) — JS
    // always writes u32 at offset 0 which is 4-byte aligned anyway.
    // `read_unaligned` was producing 0 here through the lift; the
    // multi-byte-OR pattern apparently didn't survive remill cleanly.
    let node_idx_ptr = (event_bytes_ptr as usize + event_offset::NODE_IDX as usize) as *const u32;
    let node_idx = core::ptr::read(node_idx_ptr);
    if node_idx == u32::MAX {
        core::ptr::write_unaligned(out_len_ptr as usize as *mut u32, 0);
        return 0;
    }
    // M8.5a stub: treat node_idx as fn-addr-lookup key. M8.5c
    // populates cb_fn_cache from StyledDom.
    let cb_fn_addr = node_idx as u64;
    let table_idx = __az_resolve_callback(cb_fn_addr);
    if table_idx == u32::MAX {
        core::ptr::write_unaligned(out_len_ptr as usize as *mut u32, 0);
        return 0;
    }
    // Read App's RefAny from state. M8.7 populates this from the
    // server-embedded JSON; for M8.5a it stays zero (the cb body
    // won't deref it unless framework calls are real — M8.9).
    let s = &*(state as usize as *const EventloopState);
    let (refany_lo, refany_hi) = match &s.app_data {
        // Stub access — the lift would normally GEP through Option<RefAny>.
        // For M8.5a we just pass zeros; the callback's framework calls
        // are noop'd anyway.
        _ => (0u64, 0u64),
    };
    // info_ptr is the AzCallbackInfo address. Pass the event_bytes
    // pointer for now — the callback ignores it under noop stubs.
    let update = __az_call_indirect(table_idx, refany_lo, refany_hi, event_bytes_ptr);
    // Write the Update enum value as a debugging signal at *out_len_ptr;
    // also returns it. M8.5b will replace this with real patch bytes.
    core::ptr::write_unaligned(out_len_ptr as usize as *mut u32, update);
    update
}
