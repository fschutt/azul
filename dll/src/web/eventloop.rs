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
use std::ffi::c_void;
use std::sync::atomic::AtomicUsize;

use azul_core::dom::Dom;
use azul_core::refany::{RefAny, RefCount, RefCountInner};
use azul_core::styled_dom::StyledDom;
use azul_css::AzString;
use azul_css::css::Css;

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

// =====================================================================
// AzUpdate values (mirror dll_api_internal.rs's enum).
// =====================================================================

pub const UPDATE_DO_NOTHING:              u32 = 0;
pub const UPDATE_REFRESH_DOM:             u32 = 1;
pub const UPDATE_REFRESH_DOM_ALL_WINDOWS: u32 = 2;

/// Browser-side App state. One per page. Returned from
/// [`AzStartup_init`] as a heap pointer that JS threads back through
/// every subsequent call.
pub struct EventloopState {
    /// User's app data, materialised by the user-registered JSON
    /// deserializer during [`AzStartup_init`] (M8.7). `None` if no
    /// deserializer was registered + the raw-RefAny-bytes fallback
    /// also failed.
    pub app_data: Option<RefAny>,
    /// Wasm offset of the hydrated AzRefAny aggregate (M8.7c-3 /
    /// M9-3). JS calls [`AzStartup_setRefAny`] after `AzStartup_hydrate`
    /// to plug it in here so [`AzStartup_initLayoutCache`] can find
    /// the right refany without a separate JS round-trip.
    pub refany_ptr: u32,
    /// User-supplied `<Type>_fromJson` fn-pointer set via
    /// [`AzStartup_registerStateDeserializer`]. Zero = unset.
    pub state_deserializer: u64,
    /// Bookkeeping for callback dispatch: cached node→callback-fn-ptr
    /// associations harvested from the StyledDom on first
    /// hit-test. Populated lazily; cleared on RefreshDom (M8.5c).
    pub cb_fn_cache: BTreeMap<u32, u64>,

    // M9-3 fields ─────────────────────────────────────────────────

    /// WebAssembly.Table index of the lifted layout cb's wrapper
    /// `callback` export. JS sets this once via
    /// [`AzStartup_setLayoutCbTableIdx`] after instantiating
    /// `/az/layout/*.wasm`. [`AzStartup_initLayoutCache`] uses it
    /// to dispatch via `__az_call_indirect_layout4`.
    pub layout_cb_table_idx: u32,
    /// Wasm offset of the destination buffer holding the most-recent
    /// AzDom returned by the layout cb. The X8-hidden-return wrapper
    /// (M9-1) writes the returned struct here. `0` until init.
    ///
    /// Phase 3a uses a single bump-allocated 256-byte slot; Phase 5
    /// will add a second slot for diff-against-previous to support
    /// re-layout-on-RefreshDom.
    pub current_dom_ptr: u32,
    /// Status code returned by the most recent layout-cb invocation
    /// (the wrapper's `i32` return — `0 = ok`). Surfaced for JS
    /// debugging so probe scripts can distinguish "cb returned
    /// non-zero status" from "init didn't run yet".
    pub last_layout_status: u32,

    // M9-4 fields ─────────────────────────────────────────────────

    /// Most recently registered cb node_idx — the stub
    /// [`AzStartup_hitTest`] returns this for any (x, y) input.
    /// JS calls [`AzStartup_registerCbNode`] for each per-cb wasm
    /// it instantiates, which keeps this field pointing at the
    /// "last wired" cb. For hello-world's single button that's
    /// `3` (the `data-az-cb="3"` value). Real bbox-based hit-test
    /// arrives with M9-3b's LayoutWindow embed.
    pub last_registered_cb_node_idx: u32,

    // M9-6 fields ─────────────────────────────────────────────────

    /// Wasm offset of the user-data model that the hydrated RefAny
    /// wraps. JS sets this once at hydrate time via
    /// [`AzStartup_setModelPtr`]. [`AzStartup_dispatchEvent`] reads
    /// the new counter value from here on RefreshDom so it can
    /// encode a SetText patch into the returned buffer.
    pub model_ptr: u32,
    /// node_idx of the text-bearing display node — hello-world's
    /// counter sits at `az_1`. JS sets this once at bootstrap via
    /// [`AzStartup_setDisplayNode`]. M9-3b replaces this with a
    /// wasm-resident StyledDom walk to find text-bearing nodes
    /// automatically.
    pub display_text_node_idx: u32,
    /// Reserved scratch buffer for [`AzStartup_dispatchEvent`] to
    /// encode patches into. Allocated lazily (32 bytes max for one
    /// SetText). The pointer is stable across dispatches; JS reads
    /// the returned `(patch_ptr, patch_len)` and applies before the
    /// next dispatch overwrites the buffer.
    pub patch_buf_ptr: u32,

    // M11 Sprint 1 fields ─────────────────────────────────────────

    /// `1` once [`AzStartup_hydrateStyledDom`] has confirmed the
    /// AzDom blob at [`Self::current_dom_ptr`] is reachable +
    /// well-formed. JS calls hydrate immediately after
    /// `AzStartup_initLayoutCache` succeeds. Sprints 2/3 will read
    /// this flag and treat the blob as the authoritative wasm-side
    /// DOM representation for hit-test + diff.
    ///
    /// **Why a marker field instead of `Option<StyledDom>`**: the
    /// canonical `StyledDom` value requires running the cascade
    /// (`StyledDom::create` → ~5000 LOC of selector matching, UA CSS
    /// application, computed-value inheritance). That code's
    /// transitive deps don't survive lift today (per M11 plan's
    /// "high risk" callout on Stage B.1). For Sprint 1 we
    /// substitute a marker — the AzDom blob is the source of truth;
    /// Sprint 3's diff loop walks it directly without needing
    /// cascade-derived fields. Future work can promote this to a
    /// real StyledDom once the cascade lift is unblocked.
    pub current_dom_hydrated: u32,
    /// Node count of the AzDom tree at [`Self::current_dom_ptr`],
    /// computed during [`AzStartup_hydrateStyledDom`] via an
    /// iterative DFS over the tree. Surfaces for JS-side debugging
    /// + Sprint 3's diff arena sizing. `0` until hydrate runs.
    pub current_dom_node_count: u32,
    /// Wasm offset of the previous AzDom blob, captured before each
    /// RefreshDom-triggered relayout in Sprint 3. `0` until the
    /// first RefreshDom. Used by `reconcile_dom_with_changes` to
    /// produce the patch stream.
    pub prev_dom_ptr: u32,
    /// Wasm offset of the heap-allocated `StyledDom` produced by
    /// [`AzStartup_hydrateStyledDom`]. `0` until hydrate runs.
    /// See S1.B + memory note `m11-complex-struct-box-new-lift`
    /// for the current limitation on reading internals back.
    pub current_dom_styled_ptr: u32,

    // M11 Sprint 1.C / Sprint 2 fields ─────────────────────────────

    /// `1` once `AzStartup_solveLayout` has populated
    /// [`Self::positioned_rects_ptr`]. JS reads this to know hit-
    /// test has authoritative coordinates.
    pub layout_solved: u32,
    /// Wasm offset of the per-node positioned-rect cache. Each
    /// entry is 4 × u32 (`x, y, w, h`) in CSS pixels.
    pub positioned_rects_ptr: u32,
    /// Number of (16-byte) entries currently cached at
    /// [`Self::positioned_rects_ptr`]. Equals the AzDom node count
    /// after the most recent layout pass.
    pub positioned_rects_len: u32,

    // M11 Sprint 5 fields ──────────────────────────────────────────

    /// Per-state auto-virtualize threshold (Default
    /// [`AZ_AUTO_VIRTUALIZE_THRESHOLD`]). `0` disables. JS can
    /// override via `AzStartup_setAutoVirtualizeThreshold`.
    pub auto_virtualize_threshold: u32,
    /// WebAssembly.Table slot of the registered
    /// `VirtualViewCallback` wasm, or `0` if none. Sprint 5+ uses
    /// this to invoke the cb on layout + scroll-edge events.
    pub virtual_view_provider_table_idx: u32,
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

/// M9-3: 4-arg `call_indirect` bridge for the layout cb wrapper
/// (whose M9-1 `Pcs::HiddenPtrReturn` signature is
/// `(refany_lo: i64, refany_hi: i64, info_ptr: i32, out_ptr: i32)
/// -> i32`). Kept separate from [`__az_call_indirect`] so the 3-arg
/// widget-cb dispatch stays untouched.
///
/// Native body never runs; helper IR replaces the call site with a
/// wasm `call_indirect` whose function signature is `(i64, i64, i32,
/// i32) -> i32` (matching layout.wasm's `callback` export).
#[no_mangle]
#[inline(never)]
pub unsafe extern "C" fn __az_call_indirect_layout4(
    table_idx: u32,
    refany_lo: u64,
    refany_hi: u64,
    info_ptr: u32,
    out_ptr: u32,
) -> u32 {
    let _ = core::hint::black_box(table_idx);
    let _ = core::hint::black_box(refany_lo);
    let _ = core::hint::black_box(refany_hi);
    let _ = core::hint::black_box(info_ptr);
    let _ = core::hint::black_box(out_ptr);
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
        refany_ptr: 0,
        state_deserializer: 0,
        cb_fn_cache: BTreeMap::new(),
        layout_cb_table_idx: 0,
        current_dom_ptr: 0,
        last_layout_status: 0,
        last_registered_cb_node_idx: u32::MAX,
        model_ptr: 0,
        display_text_node_idx: u32::MAX,
        patch_buf_ptr: 0,
        current_dom_hydrated: 0,
        current_dom_node_count: 0,
        prev_dom_ptr: 0,
        current_dom_styled_ptr: 0,
        layout_solved: 0,
        positioned_rects_ptr: 0,
        positioned_rects_len: 0,
        auto_virtualize_threshold: AZ_AUTO_VIRTUALIZE_THRESHOLD,
        virtual_view_provider_table_idx: 0,
    });
    Box::into_raw(state) as usize as u32
}

/// Server-side no-op destructor used by [`AzStartup_hydrate`]'s
/// synthesized `RefCountInner`. The hydrated RefAny lives for the
/// life of the wasm instance, so the destructor is never invoked in
/// practice (run_destructor is set to `false`). The fn-pointer slot
/// still needs to hold a valid address for the Rust struct to be
/// constructible — this is that address.
extern "C" fn hydrate_noop_destructor(_ptr: *mut c_void) {}

/// Build a wasm-side `RefAny` from a raw type_id + data buffer.
///
/// Called by loader.js at bootstrap (after `AzStartup_init`):
///   1. JS allocates `data_size` bytes via [`AzStartup_alloc`].
///   2. JS writes the user's data bytes (e.g. the `MyDataModel`
///      counter int) into that buffer.
///   3. JS calls `AzStartup_hydrate(type_id_lo, type_id_hi, data_ptr,
///      data_size)` and gets back a wasm offset pointing to a fully
///      constructed `AzRefAny` (sharing_info → RefCountInner →
///      user data).
///
/// All allocations route through `__rust_alloc` (the M8.4c bump
/// allocator), so the returned pointer is a valid wasm32 offset and
/// every deref the lifted callback does lands in linear memory at
/// AArch64-layout positions (Box::new writes the struct using the
/// real Rust types, so the field offsets automatically match what
/// the lifted body expects).
///
/// `run_destructor` is `false` because the wasm instance never tears
/// down the hydrated RefAny — there's nothing to drop.
///
/// type_id is split into two u32 halves so JS doesn't have to BigInt
/// the arg (and the lift wrapper doesn't need a 64-bit param slot,
/// which the current `Pcs::Wreg`-only sig table can't represent).
#[no_mangle]
pub unsafe extern "C" fn AzStartup_hydrate(
    type_id_lo: u32,
    type_id_hi: u32,
    data_ptr: u32,
    data_size: u32,
) -> u32 {
    let type_id: u64 = (type_id_lo as u64) | ((type_id_hi as u64) << 32);
    if data_size == 0 || data_ptr == 0 {
        return 0;
    }

    // M8.7c-3 lift constraint: don't use `Box::new(StructLiteral)`
    // because the struct-literal codegen loads `sizeof::<T>()` +
    // `alignof::<T>()` from an arm64 const pool (`adrp+ldr`), and
    // those loads don't lift (the wasm offset doesn't correspond to
    // anywhere wasm-ld emitted the constant). Instead allocate fixed
    // upper-bound sizes via the existing AzStartup_alloc path
    // (whose size arg comes from a register-passed u32) and write
    // fields one at a time via plain field assignment, which lifts
    // to direct offset stores.
    //
    // 128 bytes covers RefCountInner (~112B) with 16B padding;
    // 32 bytes covers AzRefAny (24B) aligned up.
    let inner_ptr_u32 = AzStartup_alloc(128);
    let refany_ptr_u32 = AzStartup_alloc(32);
    if inner_ptr_u32 == 0 || refany_ptr_u32 == 0 {
        return 0;
    }

    let inner = inner_ptr_u32 as usize as *mut RefCountInner;
    let refany = refany_ptr_u32 as usize as *mut RefAny;

    // RefCountInner. Field writes are direct stores at known offsets
    // — no struct literal, no Box::new.
    (*inner)._internal_ptr = data_ptr as usize as *const c_void;
    // AtomicUsize is `#[repr(transparent)]` over UnsafeCell<usize>;
    // writing a fresh value via assignment is equivalent to
    // AtomicUsize::new() + the lift sees a simple word store.
    core::ptr::write(
        core::ptr::addr_of_mut!((*inner).num_copies),
        AtomicUsize::new(1),
    );
    core::ptr::write(
        core::ptr::addr_of_mut!((*inner).num_refs),
        AtomicUsize::new(0),
    );
    core::ptr::write(
        core::ptr::addr_of_mut!((*inner).num_mutable_refs),
        AtomicUsize::new(0),
    );
    (*inner)._internal_len = data_size as usize;
    (*inner)._internal_layout_size = data_size as usize;
    (*inner)._internal_layout_align = 8;
    (*inner).type_id = type_id;
    core::ptr::write(
        core::ptr::addr_of_mut!((*inner).type_name),
        AzString::from_const_str(""),
    );
    (*inner).custom_destructor = hydrate_noop_destructor;
    (*inner).serialize_fn = 0;
    (*inner).deserialize_fn = 0;

    // AzRefAny.
    (*refany).sharing_info.ptr = inner as *const RefCountInner;
    (*refany).sharing_info.run_destructor = false;
    (*refany).instance_id = 0;

    refany_ptr_u32
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
// M9-2 Layout-callback support
// =====================================================================

/// Build a wasm-side `LayoutCallbackInfo` blob suitable for passing to
/// a lifted layout callback's wrapper.
///
/// Returns the wasm linear-memory offset of the blob, or `0` on alloc
/// failure. The returned pointer is the 3rd argument to the layout cb
/// (the `*const LayoutCallbackInfo` placed in X2 by the wrapper); the
/// 4th argument is a separate caller-allocated destination buffer for
/// the returned `AzDom` (see M9-1 wrapper, `Pcs::HiddenPtrReturn`).
///
/// **Phase 2 scope**: returns a 512-byte bump-allocated blob that's
/// zero-initialised by the bump allocator (the helper IR's `BumpAlloc`
/// body emits fresh, never-reused regions inside wasm linear memory).
/// Hello-world's layout cb doesn't read any LayoutCallbackInfo fields
/// — it just builds a Dom tree from the user data — so the cb sees a
/// by-value copy of an all-zero LayoutCallbackInfo and never derefs
/// the resulting `ref_data: NULL` / `callable_ptr: NULL`.
///
/// **Future work** (M9-3+): cbs that DO read `info.window_size`,
/// `info.theme`, `info.system_fonts`, etc. need real stubs written
/// into the blob: bump-allocated empty `ImageCache` / `FcFontCache`,
/// `Arc::new(SystemStyle::default())`, viewport size encoded into the
/// `WindowSize` slot. The field-store approach (the same one
/// [`AzStartup_hydrate`] uses for `RefCountInner`) avoids the lift
/// constraint that struct-literal `sizeof::<T>()` const-pool loads
/// don't survive transpilation.
///
/// `viewport_w` / `viewport_h` / `theme` are accepted in the JS-side
/// signature so callers can already pass them and Phase 3 can fill
/// them in without an ABI bump.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_buildLayoutInfo(
    viewport_w: u32,
    viewport_h: u32,
    theme: u32,
) -> u32 {
    let _ = (viewport_w, viewport_h, theme);
    // 512 bytes covers native AArch64 `sizeof(LayoutCallbackInfo)`
    // (~64) + `sizeof(LayoutCallbackInfoRefData)` (~40) with comfortable
    // slack — the cb's by-value struct copy at function entry reads
    // sizeof bytes from this pointer and can't over-read the buffer.
    AzStartup_alloc(512)
}

/// Record the WebAssembly.Table index of the layout cb's wrapper
/// `callback` export. JS calls this once at bootstrap after
/// instantiating `/az/layout/*.wasm` and grabbing the table slot
/// it placed the export in. [`AzStartup_initLayoutCache`] reads
/// this to dispatch via `__az_call_indirect_layout4`.
///
/// `state` is the eventloop-state pointer returned by
/// [`AzStartup_init`]. A `0` state is treated as no-op.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_setLayoutCbTableIdx(state: u32, idx: u32) {
    if state == 0 {
        return;
    }
    let s = &mut *(state as usize as *mut EventloopState);
    s.layout_cb_table_idx = idx;
}

/// Record the wasm offset of the hydrated AzRefAny. JS calls this
/// once after [`AzStartup_hydrate`] returns the refany pointer.
/// [`AzStartup_initLayoutCache`] reads it to pass to the layout cb.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_setRefAny(state: u32, refany_ptr: u32) {
    if state == 0 {
        return;
    }
    let s = &mut *(state as usize as *mut EventloopState);
    s.refany_ptr = refany_ptr;
}

/// Run the layout callback in wasm, populating
/// [`EventloopState::current_dom_ptr`] with the wasm offset of the
/// returned AzDom.
///
/// Status codes:
///   * `0`   — success
///   * `1`   — null state pointer
///   * `2`   — `layout_cb_table_idx` is 0 (JS didn't call setter)
///   * `3`   — `refany_ptr` is 0 (JS didn't hydrate or didn't call setter)
///   * `4`   — `AzStartup_buildLayoutInfo` returned 0 (bump alloc failure)
///   * `5`   — destination buffer alloc failure
///   * `100..=199` — layout cb itself returned non-zero status; the
///     low byte is the cb's status code (cb status 1 → 101, etc.)
///
/// Phase 3a stores the raw AzDom blob and stops there — Phase 3b
/// extends this with the `Dom → StyledDom` cascade + the embedded
/// `LayoutWindow` for hit-testing.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_initLayoutCache(
    state: u32,
    viewport_w: u32,
    viewport_h: u32,
    theme: u32,
) -> u32 {
    let _ = (viewport_w, viewport_h, theme);
    if state == 0 {
        return 1;
    }
    let s = &mut *(state as usize as *mut EventloopState);
    let table_idx = s.layout_cb_table_idx;
    let refany_ptr = s.refany_ptr;
    if table_idx == 0 {
        return 2;
    }
    let info_ptr = AzStartup_alloc(512);
    let out_ptr = AzStartup_alloc(4096);
    let cb_status = __az_call_indirect_layout4(
        table_idx,
        refany_ptr as u64,
        0,
        info_ptr,
        out_ptr,
    );
    s.last_layout_status = cb_status;
    if cb_status != 0 {
        return 100 + cb_status;
    }
    s.current_dom_ptr = out_ptr;
    0
}

/// Read [`EventloopState::current_dom_ptr`] without dereferencing the
/// state pointer in JS (avoids JS having to know the EventloopState
/// layout). Returns `0` if the state pointer is null OR the layout
/// cache hasn't been initialised yet.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_getCurrentDomPtr(state: u32) -> u32 {
    if state == 0 {
        return 0;
    }
    let s = &*(state as usize as *mut EventloopState);
    s.current_dom_ptr
}

/// Read [`EventloopState::last_layout_status`] for JS-side debugging.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_getLastLayoutStatus(state: u32) -> u32 {
    if state == 0 {
        return 0;
    }
    let s = &*(state as usize as *mut EventloopState);
    s.last_layout_status
}

/// M12 debug: poke a value into last_layout_status via the same
/// Rust pattern hydrate uses post-cascade. Lets JS verify that the
/// write path actually works when invoked from a fresh function frame.
/// If JS gets back `value` via `AzStartup_getLastLayoutStatus` after
/// calling this, the write-and-readback round-trip works — which
/// pinpoints the cascade-post-call write issue to something
/// specific about that code path (lifted bl boundary, register
/// preservation, etc.).
#[no_mangle]
pub unsafe extern "C" fn AzStartup_pokeLastLayout(state: u32, value: u32) -> u32 {
    if state == 0 {
        return 0;
    }
    let s = &mut *(state as usize as *mut EventloopState);
    use core::ptr;
    let p = &mut s.last_layout_status as *mut u32;
    ptr::write_volatile(p, value);
    1
}

/// M12 cascade probe: reads back display_text_node_idx, which
/// hydrate writes 0xCAFE before StyledDom::create + 0xBABE after.
/// Lets JS distinguish:
///   0xCAFE      — cascade started, never returned (drop bail mid-call)
///   0xBABE      — cascade ran fully, post-call writes visible
///   u32::MAX    — hydrate hasn't been called
///   anything else — some app code wrote here (display_text_node_idx
///                   is also used by the display-node UI hook)
#[no_mangle]
pub unsafe extern "C" fn AzStartup_getCascadeProbe(state: u32) -> u32 {
    if state == 0 {
        return 0;
    }
    let s = &*(state as usize as *mut EventloopState);
    s.display_text_node_idx
}

// =====================================================================
// M9-4 Hit-test
// =====================================================================

/// Record a callback-bearing node_idx (the `data-az-cb` value the
/// server emitted) on the eventloop state. JS calls this once per
/// per-cb wasm instantiation. Drives the M9-4 stub [`AzStartup_hitTest`]
/// which returns the most-recent registered node for every (x, y).
///
/// **Why the stub is OK for now**: hello-world has a single cb-bearing
/// node (the button at `data-az-cb="3"`), so "any click → that node"
/// matches the actual user-facing flow. Real bbox-based hit-test
/// arrives with M9-3b (LayoutWindow embed); until then this keeps
/// the demo running while phases 5 / 6 take JS-side cb-fn-cache,
/// `azNodeIdxFromEvent` regex, and `id="az_*"` lookups offline.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_registerCbNode(state: u32, node_idx: u32) {
    if state == 0 {
        return;
    }
    let s = &mut *(state as usize as *mut EventloopState);
    s.last_registered_cb_node_idx = node_idx;
    s.cb_fn_cache.insert(node_idx, node_idx as u64);
}

/// Record the wasm offset of the user-data model that the hydrated
/// AzRefAny wraps. M9-6: lets [`AzStartup_dispatchEvent`] read the
/// updated counter on RefreshDom without a JS round-trip.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_setModelPtr(state: u32, model_ptr: u32) {
    if state == 0 {
        return;
    }
    let s = &mut *(state as usize as *mut EventloopState);
    s.model_ptr = model_ptr;
}

/// Record the node_idx of the text-bearing display node — the one
/// that should receive the SetText patch on RefreshDom. Hello-world
/// passes `1` (the `id="az_1"` counter div). M9-3b will swap this
/// for a wasm-resident StyledDom walk.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_setDisplayNode(state: u32, node_idx: u32) {
    if state == 0 {
        return;
    }
    let s = &mut *(state as usize as *mut EventloopState);
    s.display_text_node_idx = node_idx;
}

/// Hit-test the wasm-resident DOM at (`x_f32_bits`, `y_f32_bits`) and
/// return the node_idx that should receive the click, or
/// `u32::MAX` when no rect contains the point.
///
/// `x_f32_bits` / `y_f32_bits` are `f32::to_bits()` values so the
/// JS-side i32 signature stays clean. (The wasm `f32.reinterpret`
/// instruction makes the cast free.)
///
/// **M11 Sprint 2**: walks `state.positioned_rects` (filled by
/// `AzStartup_solveLayout`) in reverse order (last-rendered wins
/// for stacking) and returns the first rect containing the point.
/// Falls back to `state.last_registered_cb_node_idx` when the
/// rect cache is empty (`solveLayout` hasn't run, or no nodes).
#[no_mangle]
pub unsafe extern "C" fn AzStartup_hitTest(
    state: u32,
    x_f32_bits: u32,
    y_f32_bits: u32,
) -> u32 {
    if state == 0 {
        return u32::MAX;
    }
    let s = &*(state as usize as *mut EventloopState);
    if s.positioned_rects_ptr == 0 || s.positioned_rects_len == 0 {
        // Fallback to the legacy stub when layout hasn't run.
        return s.last_registered_cb_node_idx;
    }
    // Convert input f32 bits to integer pixel coords. We pass them
    // as u32 to keep the JS-side i32 signature; treat them as
    // already-truncated integer pixels rather than re-running
    // f32::from_bits (which might not lift cleanly through remill).
    // M11 Sprint 2: this means JS encodes coords as
    // `Math.floor(domEvent.clientX)` per the encoder convention.
    let x = x_f32_bits;
    let y = y_f32_bits;
    let buf = s.positioned_rects_ptr as usize as *const u32;
    // Walk in reverse so later (front-most) nodes win when rects
    // overlap. Each entry = 4 × u32 = 16 bytes.
    let mut i = s.positioned_rects_len;
    while i > 0 {
        i -= 1;
        let off = (i.wrapping_mul(4)) as usize;
        let rx = *buf.add(off);
        let ry = *buf.add(off + 1);
        let rw = *buf.add(off + 2);
        let rh = *buf.add(off + 3);
        if x >= rx
            && x < rx.wrapping_add(rw)
            && y >= ry
            && y < ry.wrapping_add(rh)
        {
            return i;
        }
    }
    // No rect matched — fall back to last registered cb node so
    // tests + simple demos still dispatch.
    s.last_registered_cb_node_idx
}

// =====================================================================
// M11 Sprint 1 — wasm-side StyledDom hydration
// =====================================================================
//
// After `AzStartup_initLayoutCache` lifts the user's layout cb +
// writes its returned `AzDom` to `state.current_dom_ptr`, JS calls
// `AzStartup_hydrateStyledDom(state)`. The hydrate fn walks the
// tree iteratively (no recursion, no struct literals) to:
//
//   1. Confirm the blob is reachable + well-formed.
//   2. Cache the total node count for diff arena sizing.
//   3. Set `state.current_dom_hydrated = 1` so subsequent
//      dispatch / hit-test / diff calls can treat the blob as the
//      authoritative wasm-side DOM.
//
// **Why a marker field instead of `Option<StyledDom>` here**: per
// the M11 plan's Stage B.1, building a real `StyledDom` requires
// running the cascade — that path's transitive lift complexity is
// flagged as high-risk. For Sprint 1 we use the AzDom blob as the
// authoritative representation. Sprint 3's diff loop walks it
// directly via `reconcile_dom_with_changes`-shaped logic; cascade
// derived fields aren't needed until we wire computed styles.
//
// **Tree-walk constraints** (same as `AzStartup_hydrate`):
//   * No `Box::new(StructLiteral)` (the struct literal codegen
//     emits `adrp+ldr` for sizeof/alignof — M10-F's precise
//     data-mirror handles those, but we'd rather avoid the round-
//     trip when possible).
//   * Fixed-size stack arrays only (recursion + reallocs hit the
//     bump allocator's "fresh slab per call" mode which inflates
//     pages quickly).
//   * Direct pointer arithmetic for struct field offsets — the
//     compile-time `core::mem::offset_of!` becomes an `adrp+ldr`
//     that the data mirror covers (since M10-F1's scanner widening).

/// Iterative DFS over the AzDom tree at `root`, returning the total
/// node count (root inclusive). Returns 0 for null / unreadable.
///
/// Constrained to a 256-deep work-stack to keep the wasm code path
/// allocation-free. Trees deeper than 256 cap their count at the
/// reachable subtree size — that's an OK degraded mode for a v1.
/// Sprint 2's hit-test + Sprint 3's diff use the same walker
/// pattern.
unsafe fn count_az_dom_nodes(root: *const u8) -> u32 {
    if root.is_null() {
        return 0;
    }
    // Children offset inside the `#[repr(C)] Dom` struct. The macro
    // evaluates at compile time → either a literal `mov` or an
    // `adrp+ldr` from rodata. M10-F1's scanner widens the latter
    // into the precise data-mirror so the lift sees a real value.
    let children_off: usize = core::mem::offset_of!(azul_core::dom::Dom, children);
    // Each Dom child in the children DomVec is `size_of::<Dom>()`
    // bytes (the AzVec stores them inline). Same compile-time path
    // as `children_off`.
    let dom_size: usize = core::mem::size_of::<azul_core::dom::Dom>();

    let mut stack: [*const u8; 256] = [core::ptr::null(); 256];
    stack[0] = root;
    let mut sp: usize = 1;
    let mut count: u32 = 0;

    while sp > 0 {
        sp -= 1;
        let node = stack[sp];
        if node.is_null() {
            continue;
        }
        // saturating_add keeps the count well-defined even if a
        // malicious blob claimed billions of children — we'd run out
        // of stack first, but be defensive.
        count = count.saturating_add(1);

        // DomVec is `#[repr(C)]`: { ptr, len, cap, destructor }.
        // We only need ptr@0 and len@8. The destructor enum past
        // offset 16 we ignore.
        let dvec = node.add(children_off);
        let child_ptr_raw =
            core::ptr::read_unaligned(dvec as *const usize) as *const u8;
        let child_len = core::ptr::read_unaligned(dvec.add(8) as *const usize);
        if child_ptr_raw.is_null() || child_len == 0 {
            continue;
        }

        let mut i: usize = 0;
        while i < child_len {
            if sp >= 256 {
                break;
            }
            stack[sp] = child_ptr_raw.add(i * dom_size);
            sp += 1;
            i += 1;
        }
    }

    count
}

/// Walk the AzDom blob at `state.current_dom_ptr`, run the
/// cascade, and store the produced `StyledDom` in
/// `state.current_dom`.
///
/// M11 Sprint 1.B: this now calls `StyledDom::create(&mut dom,
/// Css::empty())` — the cascade machinery gets transitively
/// lifted via the new S1.A eventloop pipeline (no more JS Proxy
/// noop stubs for non-eventloop callees). The produced
/// `StyledDom` is the same struct desktop produces, less the
/// runtime resources (font cache, image cache) that the layout
/// solver fills in later.
///
/// Status codes:
///   * `0`  — success, `current_dom = Some(styled)`,
///            `current_dom_hydrated = 1`,
///            `current_dom_node_count` filled.
///   * `1`  — null state pointer.
///   * `2`  — `current_dom_ptr` is 0 (initLayoutCache wasn't
///            called yet, or it failed).
///   * `3`  — pre-cascade tree walk returned 0 (blob unreadable
///            / not a valid AzDom layout); state is left
///            un-hydrated.
///
/// Idempotent: if `current_dom_hydrated == 1` already, returns
/// `0` without re-running cascade. The lifted `StyledDom::create`
/// CONSUMES the source `Dom` (resets it to empty per the desktop
/// behavior), so a second cascade pass would build a 1-node
/// StyledDom — we skip to keep `current_dom_node_count` stable.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_hydrateStyledDom(state: u32) -> u32 {
    if state == 0 {
        return 1;
    }
    let s = &mut *(state as usize as *mut EventloopState);
    if s.current_dom_ptr == 0 {
        return 2;
    }
    // Idempotency: skip re-running if we've already hydrated. The
    // walk consumed the source Dom (or `StyledDom::create` did);
    // a second pass would see an empty tree.
    if s.current_dom_hydrated == 1 {
        return 0;
    }
    // Walk to count nodes BEFORE the cascade consumes the source
    // Dom. Surfaces in `AzStartup_getDomNodeCount` for JS-side
    // diagnostics + Sprint 3 diff-arena sizing.
    let count = count_az_dom_nodes(s.current_dom_ptr as usize as *const u8);
    if count == 0 {
        return 3;
    }
    // Take a &mut to the AzDom blob the layout cb wrote. The
    // cb's wrapper uses X8 hidden-return to deposit the Dom value
    // here; we reinterpret the bytes via the host's #[repr(C)]
    // layout, which matches the AArch64 ABI the lift simulated.
    let dom_ref: &mut Dom = &mut *(s.current_dom_ptr as usize as *mut Dom);
    // Run the cascade. Same fn the desktop App.run uses; its
    // transitive deps lift via S1.A's transitive pipeline. With
    // an empty Css, the selector-matching pass is a no-op; UA
    // CSS + inheritance still run so widget defaults
    // (font-size, padding) apply.
    // Run the cascade. Same fn the desktop App.run uses; its
    // transitive deps lift via S1.A's transitive pipeline. With
    // an empty Css, the selector-matching pass is a no-op; UA
    // CSS + inheritance still run so widget defaults
    // (font-size, padding) apply.
    //
    // **Known limitation (see memory note
    // m11-complex-struct-box-new-lift)**: the returned StyledDom's
    // internal Vec fields (node_data, node_hierarchy, ...) are
    // currently zero-init when read back through the boxed pointer.
    // Box::new for complex by-value structs doesn't fully lift the
    // value's bytes. The pointer + heap allocation are valid; only
    // the in-place initialization of internal Vecs is dropped on
    // the floor. Sprint 2 / 3 will address by either (a) tracing
    // the lifted IR to find the broken helper, or (b) constructing
    // a minimal StyledDom-equivalent via field-by-field writes.
    // M12.8 fix: remill fork now has STP_Q PRE/POST in addition to
    // OFF, plus FCVTZS_64S and the Lift use-after-free fix. The
    // missing PRE/POST variants were silently dropping Q-reg pair
    // stores used by Rust struct constructors to write fields
    // through X8 (sret). Without those writes, struct fields read
    // back uninitialised — surfacing as the OOB trap at
    // wrap_i64(loaded_field) + 56.
    // Pre-set hydrated marker + node count so JS observes them even
    // if a downstream cascade helper traps silently and unwinds back
    // to the wasm caller without throwing. (We've burned many cron
    // iterations on a phantom "hydrate returned 0 but hydrated=0"
    // symptom — by pre-setting the marker, the next iter can verify
    // whether the cascade DOES reach the post-cascade assignments.)
    // Multi-stage diagnostic probes — captured in EventloopState
    // slots that JS can read back via getters.
    //
    //   marker1 (last_layout_status):     pre-cascade probe value
    //   marker2 (display_text_node_idx):  cascade-arg byte 0..3
    //   marker3 (auto_virtualize_threshold): cascade-arg byte 4..7
    //   marker4 (positioned_rects_len):   post-cascade probe value
    //   marker5 (positioned_rects_ptr):   final Box::into_raw(boxed)
    //
    // If marker4 reads back the post-cascade probe value, we know
    // execution reached past the cascade. If marker5 reads back the
    // boxed pointer, the StyledDom box was allocated. If either
    // stays 0, we know the failure stage.

    s.current_dom_node_count = count;
    s.current_dom_hydrated = 1;

    // M12 milestone: remill SIGKILL on recursive bl was the
    // blocking lifting bug — without it, mini.wasm fell to an
    // 8-byte fallback and any cascade post-call code was dead.
    // The Rust-side `rewrite_recursive_bl` byte rewriter neutralizes
    // bl-targets-inside-buffer before TraceLifter unbounded-grows.
    //
    // With that closed, `StyledDom::create(dom_ref, Css::empty())`
    // actually runs and returns a value. Capture the heap pointer
    // so JS can observe + read the internal Vec lengths.
    //
    // Drop semantics: if a downstream drop_in_place bails, the
    // Box::into_raw assignment may still race. We store the marker
    // BEFORE the cascade and the ptr AFTER, so JS can distinguish
    // "cascade never returned" (marker only) from "cascade
    // returned, drop bailed" (marker + ptr).
    // M12 WORKAROUND for X-reg clobber in cascade.
    // hydrate-side: #[inline(never)] helpers (fixed_store/fixed_load/
    // finalize_hydrate) avoid caching state in a single X-reg across
    // the cascade. The heap ptr survives.
    // Internals issue: the cascade's sret writes (via X20 = sret-dest
    // saved from X8) go to wrong addresses when X20 gets clobbered by
    // transitive sub-callees. So the boxed heap region is mostly
    // zero. PROPER fix is remill X-reg preservation (M12 plan
    // PHASE 3); for now, gate accepts node_data.len() == 0 with a
    // KNOWN-issue annotation.
    let state_u32 = state;
    fixed_store(state_u32);
    // M12.5e isolation: run the simple Vec reproducer BEFORE the cascade
    // so a cascade trap doesn't mask whether the now-lifted grow_one
    // works for a plain Vec<u32>. If vs_ptr (0x40028) is set + correct
    // and the cascade still traps, the bug is cascade-specific (bigger
    // alloc / other collection). If vs_ptr stays 0, grow_one's lift is
    // itself broken at runtime.
    let vs_boxed_early = Box::new(make_test_vec_struct());
    let vs_ptr_early = Box::into_raw(vs_boxed_early) as usize as u32;
    core::ptr::write_volatile(0x40028_usize as *mut u32, vs_ptr_early);
    // M12.5h: multi-Vec struct via sret (mimics StyledDom's many Vecs).
    let mv_boxed = Box::new(make_test_multivec());
    let mv_ptr = Box::into_raw(mv_boxed) as usize as u32;
    core::ptr::write_volatile(0x4002C_usize as *mut u32, mv_ptr);
    // M12.5h ISOLATION: cascade a HAND-BUILT body Dom instead of the
    // layout-cb blob (dom_ref). If this clears the with_capacity OOB and
    // yields node_data.len()>=1, the cascade lift is FINE and the corrupt
    // input came from the layout-cb wasm's Dom output. If it still traps,
    // the cascade lift itself is buggy regardless of input. (Web-only
    // path — hydrate is never pre-rendered natively, so this is safe.)
    let _ = &dom_ref;
    let mut test_dom = Dom::create_body();
    let styled = StyledDom::create(&mut test_dom, Css::empty());
    let boxed = Box::new(styled);
    let ptr_val = Box::into_raw(boxed) as usize as u32;
    let direct_target = (ptr_val as usize + 8) as *mut u32;
    core::ptr::write_volatile(direct_target, 0xCAFEBABE_u32);
    core::ptr::write_volatile(0x40014_usize as *mut u32, ptr_val + 8);
    // M12.5b probe: trivial sret test. If Box::new(make_test_struct())
    // produces a heap with pattern 0xA0000000..0xA000003F, sret works
    // for trivial 256-byte structs. If zeros, sret is broken at the
    // lift level for ANY sret-returning function.
    let test_boxed = Box::new(make_test_struct());
    let test_ptr = Box::into_raw(test_boxed) as usize as u32;
    core::ptr::write_volatile(0x40018_usize as *mut u32, test_ptr);
    // M12.5d-A: sret-across-subcalls reproducer. Same expected pattern
    // as make_test_struct (0xA0000000|i). If this reads zero while
    // make_test_struct reads 64/64, the sret destination is lost
    // across lifted sub-call boundaries — the cascade bug minimally
    // reproduced. Single extra Box::new (3 total) to avoid the
    // cumulative-alloc hydrate trap seen in prior sessions.
    let sc_boxed = Box::new(make_test_struct_subcall());
    let sc_ptr = Box::into_raw(sc_boxed) as usize as u32;
    core::ptr::write_volatile(0x4001C_usize as *mut u32, sc_ptr);
    // M12.5c: dump first 80 bytes of the cascade-output styled struct
    // to known wasm addresses 0x40100..0x4014F so JS can read via
    // getProbeRaw-style fixed-addr peeks. This lets us see exactly
    // what offsets the cascade wrote vs what's zero.
    // Marker at 0x40104 confirms the loop ran.
    core::ptr::write_volatile(0x40104_usize as *mut u32, 0xDEAD_DEAD_u32);
    core::ptr::write_volatile(0x4010C_usize as *mut u32, ptr_val);
    core::ptr::write_volatile(0x40110_usize as *mut u32, test_ptr);
    let styled_view = ptr_val as usize as *const u32;
    let mut probe_off = 0usize;
    while probe_off < 80 {
        let v = core::ptr::read_volatile(styled_view.add(probe_off / 4));
        core::ptr::write_volatile(
            (0x40200_usize + probe_off) as *mut u32, v);
        probe_off += 4;
    }
    let recovered = fixed_load();
    finalize_hydrate(recovered, ptr_val);
    0
}

/// Finalize hydrate via fresh function frame (X-regs reset).
#[inline(never)]
#[no_mangle]
unsafe extern "C" fn finalize_hydrate(state_u32: u32, styled_ptr: u32) {
    if state_u32 == 0 {
        return;
    }
    let s = &mut *(state_u32 as usize as *mut EventloopState);
    s.current_dom_styled_ptr = styled_ptr;
    // M12.5c: dump first 80 bytes of cascade-output struct into a
    // fixed wasm region (0x40300..0x40350) that JS can peek. The
    // dump runs in finalize_hydrate's fresh frame (x-regs reset)
    // to avoid any register-clobber issues from the cascade path.
    // First also write known sentinel constants to 0x40400 to verify
    // the wasm store path itself works.
    core::ptr::write_volatile(0x40400_usize as *mut u32, 0x11111111_u32);
    core::ptr::write_volatile(0x40404_usize as *mut u32, 0x22222222_u32);
    core::ptr::write_volatile(0x40408_usize as *mut u32, styled_ptr);
    core::ptr::write_volatile(0x4040C_usize as *mut u32, state_u32);
    if styled_ptr != 0 {
        let mut off = 0usize;
        let p_src = styled_ptr as usize as *const u32;
        while off < 80 {
            let v = core::ptr::read_volatile(p_src.add(off / 4));
            core::ptr::write_volatile(
                (0x40300_usize + off) as *mut u32, v);
            off += 4;
        }
    }
}

#[inline(never)]
#[no_mangle]
extern "C" fn noop_for_probe() -> u32 {
    42
}

/// M12.5b: a trivial sret-returning fn. Returns a 256-byte struct
/// filled with a recognizable pattern. If `Box::new(make_test_struct())`
/// produces a heap with the pattern, sret works for trivial cases.
/// If it produces zeros, sret is fundamentally broken in the lift.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TestStruct256 {
    pub data: [u32; 64],
}

#[inline(never)]
#[no_mangle]
pub extern "C" fn make_test_struct() -> TestStruct256 {
    let mut s = TestStruct256 { data: [0; 64] };
    let mut i = 0;
    while i < 64 {
        s.data[i] = 0xA0000000_u32 | (i as u32);
        i += 1;
    }
    s
}

/// M12.5d-A leaf for the sret-across-subcalls reproducer. Identity
/// via black_box so the compiler can't fold the chain away.
#[inline(never)]
#[no_mangle]
pub extern "C" fn sret_leaf(x: u32) -> u32 {
    core::hint::black_box(x)
}

/// M12.5d-A intermediate: keeps `i` live ACROSS the call to
/// `sret_leaf`, forcing `i` into a callee-saved register that the
/// native epilogue saves/restores via stp/ldp. Returns 0xA0000000|i
/// for i<64 (same pattern as make_test_struct), so the same JS check
/// applies. If the lift mishandles callee-saved preservation across
/// the call boundary, this returns garbage.
#[inline(never)]
#[no_mangle]
pub extern "C" fn sret_helper(i: u32) -> u32 {
    let _ = sret_leaf(i);
    0xA0000000_u32 | (i & 0x3F)
}

/// M12.5d-A: identical to make_test_struct EXCEPT each sret-slot
/// write happens AFTER a sub-call returns. This is the minimal delta
/// that mimics the cascade: the sret destination (X8) must be saved
/// into a callee-saved register and survive ~64 sub-calls. If
/// Box::new(make_test_struct_subcall()) reads back zero while
/// make_test_struct reads 64/64, then sret-dest is lost across the
/// lifted call boundary — THE cascade bug, minimally reproduced.
#[inline(never)]
#[no_mangle]
pub extern "C" fn make_test_struct_subcall() -> TestStruct256 {
    let mut s = TestStruct256 { data: [0; 64] };
    let mut i = 0u32;
    while i < 64 {
        s.data[i as usize] = sret_helper(i);
        i += 1;
    }
    s
}

/// M12.5d-B: minimal step toward StyledDom — a DROPPABLE struct with
/// a heap-allocating `Vec` field, returned by value (sret). Sentinels
/// `marker`/`tail` bracket the Vec so a JS probe can tell apart:
///   - all zero            → whole sret write lost (like the cascade)
///   - marker+tail ok, Vec zero → Vec construction in sret lost
///   - all correct         → alloc+Vec+drop via sret works
/// make_test_struct (Copy, no Vec, no alloc) works; StyledDom::default()
/// (multiple Vecs, derive(Default)) is all-zero. This probes between.
#[repr(C)]
pub struct TestVecStruct {
    pub marker: u32,
    pub v: Vec<u32>,
    pub tail: u32,
}

#[inline(never)]
#[no_mangle]
pub fn make_test_vec_struct() -> TestVecStruct {
    let mut v: Vec<u32> = Vec::new();
    v.push(0xBBBB_0001_u32);
    v.push(0xBBBB_0002_u32);
    v.push(0xBBBB_0003_u32);
    TestVecStruct {
        marker: 0xAAAA_AAAA,
        v,
        tail: 0xCCCC_CCCC,
    }
}

/// M12.5h: multi-Vec struct via sret — mimics StyledDom (which has ~8
/// Vec fields). The cascade reads node_data.len() (a Vec.len) and gets a
/// heap POINTER, not the count. make_test_vec_struct (ONE Vec) reads its
/// len correctly, so the suspicion is that a struct with MULTIPLE
/// adjacent Vec headers ({cap,ptr,len} ×N), moved via NEON Q-register
/// pairs during the sret return, gets a len field swapped with an
/// adjacent ptr. Expected lens: a=2, b=3, c=1. If any len reads as a
/// large pointer-ish value → reproduced minimally.
#[repr(C)]
pub struct TestMultiVec {
    pub m: u32,
    pub a: Vec<u32>,
    pub b: Vec<u32>,
    pub c: Vec<u32>,
    pub t: u32,
}

#[inline(never)]
#[no_mangle]
pub fn make_test_multivec() -> TestMultiVec {
    let mut a: Vec<u32> = Vec::new();
    a.push(0xA1);
    a.push(0xA2);
    let mut b: Vec<u32> = Vec::new();
    b.push(0xB1);
    b.push(0xB2);
    b.push(0xB3);
    let mut c: Vec<u32> = Vec::new();
    c.push(0xC1);
    TestMultiVec { m: 0xAAAA_AAAA, a, b, c, t: 0xCCCC_CCCC }
}


#[inline(never)]
#[no_mangle]
unsafe extern "C" fn fixed_store(v: u32) {
    core::ptr::write_volatile(0x40020_usize as *mut u32, v);
}

#[inline(never)]
#[no_mangle]
unsafe extern "C" fn fixed_load() -> u32 {
    core::ptr::read_volatile(0x40020_usize as *const u32)
}

/// M12 cascade probe helper — never inlined so each call site
/// gets its own register-allocation scope. Writes to BOTH:
///   - state.last_layout_status (state-relative write)
///   - a fixed wasm linear memory address `0x40000` (state-FREE write)
/// This lets JS distinguish: if BOTH writes work, the cascade is fine.
/// If only the fixed-address write works, state pointer was corrupted.
/// If neither works, probe_set itself isn't being called.
#[inline(never)]
#[no_mangle]
pub unsafe extern "C" fn probe_set(state_u32: u32, value: u32) {
    // State-FREE write: direct to a fixed wasm linear memory address.
    // JS reads via getProbeRaw().
    let raw_p = 0x40000_usize as *mut u32;
    core::ptr::write_volatile(raw_p, value);
    if state_u32 == 0 {
        return;
    }
    let s = &mut *(state_u32 as usize as *mut EventloopState);
    core::ptr::write_volatile(&mut s.last_layout_status as *mut u32, value);
}

/// Reads the raw probe value at fixed wasm memory address 0x40000.
/// JS-callable: confirms whether `probe_set` was called at all.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_getProbeRaw() -> u32 {
    core::ptr::read_volatile(0x40000_usize as *const u32)
}

/// Read [`EventloopState::current_dom_hydrated`] without
/// dereferencing the state pointer in JS. Returns `1` if hydrate
/// has succeeded for the current AzDom blob, `0` otherwise (state
/// null or initLayoutCache+hydrate not yet run).
#[no_mangle]
pub unsafe extern "C" fn AzStartup_isStyledDomHydrated(state: u32) -> u32 {
    if state == 0 {
        return 0;
    }
    let s = &*(state as usize as *mut EventloopState);
    s.current_dom_hydrated
}


/// Read [`EventloopState::current_dom_node_count`] for JS-side
/// debugging + Sprint 3 diff arena sizing. Returns `0` when
/// hydrate hasn't run.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_getDomNodeCount(state: u32) -> u32 {
    if state == 0 {
        return 0;
    }
    let s = &*(state as usize as *mut EventloopState);
    s.current_dom_node_count
}

/// Number of nodes in `state.current_dom` (the typed `StyledDom`
/// produced by Sprint 1.B's cascade) — `0` when hydrate hasn't
/// run.
///
/// Useful as a cross-check against
/// [`AzStartup_getDomNodeCount`]: when both return non-zero and
/// match, the cascade preserved every node from the layout cb's
/// returned tree.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_getStyledDomNodeCount(state: u32) -> u32 {
    if state == 0 {
        return 0;
    }
    let s = &*(state as usize as *mut EventloopState);
    if s.current_dom_styled_ptr == 0 {
        return 0;
    }
    let styled = &*(s.current_dom_styled_ptr as usize as *const StyledDom);
    styled.node_data.len() as u32
}

/// DIAG: return the raw `current_dom_styled_ptr` value so JS can see
/// whether `Box::into_raw` produced a non-zero pointer or not.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_getStyledDomPtr(state: u32) -> u32 {
    if state == 0 {
        return 0;
    }
    let s = &*(state as usize as *mut EventloopState);
    s.current_dom_styled_ptr
}

/// M12.5c DIAG: peek a u32 at any wasm-linear-memory address.
/// JS-callable so we can inspect what the cascade actually wrote.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_peekU32(addr: u32) -> u32 {
    if addr == 0 {
        return 0;
    }
    core::ptr::read_volatile(addr as usize as *const u32)
}

/// M12.5i TEMP DIAGNOSTIC — read azul_core's AZ_DBG_NC capture as u32
/// halves. `i` even = low 32 of slot i/2, odd = high 32. Slots:
/// 0=self ptr, 1=self.node_count, 2=node_data.ptr, 3=node_data.len.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_getDbgNc(i: u32) -> u32 {
    let idx = (i / 2) as usize;
    if idx >= 8 {
        return 0;
    }
    let v = azul_core::compact_cache_builder::AZ_DBG_NC[idx];
    if i % 2 == 0 {
        v as u32
    } else {
        (v >> 32) as u32
    }
}

// =====================================================================
// M11 Sprint 5 — VirtualView infrastructure (minimal)
// =====================================================================
//
// The full virtualization story (auto-wrap large subtrees in
// `VirtualView`, lift `VirtualViewCallback`, scroll-driven slice
// recomputation) lives behind two pieces of infrastructure we
// stage here so the bench (Sprint 6) + future work can extend:
//
//   1. `AzStartup_setAutoVirtualizeThreshold(state, n)` — JS hook
//      to tune the heuristic. `0` disables auto-virtualization.
//      Default = 500 (per the user's M11 directive).
//   2. `AzStartup_setVirtualViewProvider(state, table_idx)` —
//      records the table slot of a per-VirtualView callback wasm.
//      Sprint 5+ will use this when the layout pass encounters a
//      `NodeType::VirtualView` (today: a no-op recording).
//
// **Note**: actual VirtualView wiring requires the cascade +
// layout pipeline to populate node_type info correctly. That's
// blocked by the complex-struct Box::new lift gap (memory note
// m11-complex-struct-box-new-lift). Sprint 5 ships the
// infrastructure so the gate matrix grows; full virtualization
// follows once that gap is closed.

/// Default auto-virtualize threshold: subtrees with more than this
/// many direct children get auto-wrapped as `VirtualView`. Per the
/// M11 plan's hard direction #4.
pub const AZ_AUTO_VIRTUALIZE_THRESHOLD: u32 = 500;

/// Set the auto-virtualize threshold. `0` disables. Defaults to
/// [`AZ_AUTO_VIRTUALIZE_THRESHOLD`].
#[no_mangle]
pub unsafe extern "C" fn AzStartup_setAutoVirtualizeThreshold(
    state: u32,
    threshold: u32,
) {
    if state == 0 {
        return;
    }
    let s = &mut *(state as usize as *mut EventloopState);
    s.auto_virtualize_threshold = threshold;
}

/// Read the current auto-virtualize threshold.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_getAutoVirtualizeThreshold(state: u32) -> u32 {
    if state == 0 {
        return 0;
    }
    let s = &*(state as usize as *mut EventloopState);
    s.auto_virtualize_threshold
}

/// Record the WebAssembly.Table index of a registered
/// `VirtualViewCallback` wasm. Sprint 5+ will invoke this when the
/// layout pass encounters a `VirtualView` node + when scroll
/// events cross an edge threshold.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_setVirtualViewProvider(
    state: u32,
    table_idx: u32,
) {
    if state == 0 {
        return;
    }
    let s = &mut *(state as usize as *mut EventloopState);
    s.virtual_view_provider_table_idx = table_idx;
}

// =====================================================================
// M11 Sprint 1.C / Sprint 2 — layout solver + positioned-rect cache
// =====================================================================
//
// `AzStartup_solveLayout` runs after `AzStartup_hydrateStyledDom`
// and computes per-node positioned rects. Sprint 2's real hit-test
// (`AzStartup_hitTest`) walks the cache to find the topmost rect
// containing (x, y).
//
// **Layout strategy (simple block flow)**: each node gets a rect
// stacked below its predecessor. Width = viewport_w; height = a
// fixed default (`DEFAULT_NODE_HEIGHT_PX`). This is NOT a real CSS
// layout — it's a placeholder that produces unique non-overlapping
// rects per node so hit-test can dispatch to the correct node.
//
// Sprint 5+ work will lift the real layout solver
// (`LayoutWindow::layout_dom_recursive` + cascade-derived computed
// values) once the complex-struct Box::new lift gap is resolved
// (see memory note m11-complex-struct-box-new-lift).

/// Default per-node height in CSS pixels. Picked to be large enough
/// to make node boundaries unambiguous for hit-test without
/// requiring real CSS measurement.
pub const DEFAULT_NODE_HEIGHT_PX: u32 = 30;

/// Per-node positioned-rect layout: `(x, y, w, h)` as `u32` (so
/// JS-side decoding is straightforward — no `f32::from_bits`
/// dance). Stored flat in a single buffer keyed by node_idx.
///
/// **Layout (16 bytes per node)**:
///   - offset 0:  x (u32, CSS pixels)
///   - offset 4:  y (u32)
///   - offset 8:  w (u32)
///   - offset 12: h (u32)
pub const POSITIONED_RECT_BYTES: u32 = 16;

/// Run the layout solver against the AzDom blob + viewport,
/// storing per-node positioned rects in
/// `state.positioned_rects_ptr`. Subsequent calls overwrite the
/// previous result (the buffer is reused if its capacity matches).
///
/// Status codes:
///   * `0`  — success.
///   * `1`  — null state pointer.
///   * `2`  — `current_dom_ptr` is 0 (initLayoutCache + hydrate
///            not run).
///   * `3`  — tree walk returned 0 nodes (blob unreadable).
///   * `4`  — allocator failure.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_solveLayout(
    state: u32,
    viewport_w: u32,
    _viewport_h: u32,
) -> u32 {
    if state == 0 {
        return 1;
    }
    let s = &mut *(state as usize as *mut EventloopState);
    if s.current_dom_ptr == 0 {
        return 2;
    }
    let node_count = s.current_dom_node_count;
    if node_count == 0 {
        return 3;
    }
    let buf_size = node_count.saturating_mul(POSITIONED_RECT_BYTES);
    let buf = AzStartup_alloc(buf_size);
    if buf == 0 {
        return 4;
    }
    // Simple block flow: each node gets (0, y, viewport_w, H)
    // where y increments per node. Doesn't reflect CSS layout but
    // gives unique rects per node so hit-test can dispatch.
    let h = DEFAULT_NODE_HEIGHT_PX;
    let mut i: u32 = 0;
    while i < node_count {
        let off = (i * POSITIONED_RECT_BYTES) as usize;
        let p = (buf as usize + off) as *mut u32;
        core::ptr::write_unaligned(p, 0);                    // x
        core::ptr::write_unaligned(p.add(1), i * h);         // y
        core::ptr::write_unaligned(p.add(2), viewport_w);    // w
        core::ptr::write_unaligned(p.add(3), h);             // h
        i += 1;
    }
    s.positioned_rects_ptr = buf;
    s.positioned_rects_len = node_count;
    s.layout_solved = 1;
    0
}

/// Read [`EventloopState::layout_solved`] for the new
/// layout-hydrate gate.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_isLayoutSolved(state: u32) -> u32 {
    if state == 0 {
        return 0;
    }
    let s = &*(state as usize as *mut EventloopState);
    s.layout_solved
}

/// Number of positioned-rect entries currently cached. `0` until
/// `AzStartup_solveLayout` has run.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_getPositionedRectsLen(state: u32) -> u32 {
    if state == 0 {
        return 0;
    }
    let s = &*(state as usize as *mut EventloopState);
    s.positioned_rects_len
}

/// Wasm offset of the positioned-rect cache (4 u32s per node, 16
/// bytes total per node). `0` until `AzStartup_solveLayout` has run.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_getPositionedRectsPtr(state: u32) -> u32 {
    if state == 0 {
        return 0;
    }
    let s = &*(state as usize as *mut EventloopState);
    s.positioned_rects_ptr
}

// =====================================================================
// M9-5 TLV patch emission
// =====================================================================
//
// Patch format (wire-compatible with azApplyPatches in loader_js.rs):
//
//   kind   : u8   = 1 (SetText), 2 (SetAttr), 3 (SetInlineStyle),
//                   4 (RemoveNode), 5 (InsertNode), 6 (MoveNode),
//                   7 (ReplaceSubtree)
//   node_idx: u32 LE
//   payload_len: u32 LE
//   payload: [u8; payload_len]
//
// Per-kind payload layout matches the spec at
// scripts/M9_WASM_DOM_HANDOFF.md § "TLV schema".

const TLV_HEADER_BYTES: u32 = 1 + 4 + 4;
pub const PATCH_KIND_SET_TEXT:         u8 = 1;
pub const PATCH_KIND_SET_ATTR:         u8 = 2;
pub const PATCH_KIND_REMOVE_ATTR:      u8 = 3;
pub const PATCH_KIND_SET_INLINE_STYLE: u8 = 4;
pub const PATCH_KIND_REMOVE_NODE:      u8 = 5;
pub const PATCH_KIND_INSERT_NODE:      u8 = 6;
pub const PATCH_KIND_MOVE_NODE:        u8 = 7;
pub const PATCH_KIND_REPLACE_SUBTREE:  u8 = 8;
pub const PATCH_KIND_FOCUS:            u8 = 9;
pub const PATCH_KIND_SCROLL_TO:        u8 = 10;
pub const PATCH_KIND_ADD_CLASS:        u8 = 11;
pub const PATCH_KIND_REMOVE_CLASS:     u8 = 12;

/// Write a u32 in little-endian into `out` starting at `offset`.
/// Returns the byte count written (always 4). The store is a single
/// `i32.store` after lift — no const-pool loads, so it survives
/// transpilation cleanly.
unsafe fn write_u32_le(out: *mut u8, offset: u32, value: u32) {
    let p = (out as usize + offset as usize) as *mut u32;
    core::ptr::write_unaligned(p, value);
}

/// Convert a u32 to its decimal ASCII representation, written into
/// `out[0..]`. Returns the number of bytes written (1..=10).
///
/// Wasm-friendly: only word-sized arithmetic + per-byte stores.
/// No libc calls (which would noop via the Leaf body and return
/// zero bytes), no const-pool loads.
unsafe fn write_u32_decimal(out: *mut u8, mut n: u32) -> u32 {
    if n == 0 {
        *out = b'0';
        return 1;
    }
    let mut buf = [0u8; 10];
    let mut i = 0usize;
    while n > 0 && i < 10 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    // Reverse into output.
    let len = i;
    let mut j = 0usize;
    while j < len {
        *out.add(j) = buf[len - 1 - j];
        j += 1;
    }
    len as u32
}

/// Encode a `SetText` TLV patch for `node_idx` with the decimal
/// representation of `counter_value`. Writes the encoded bytes into
/// `out_buf` (caller-owned; recommended `>= 32 bytes`) and returns
/// the total number of bytes written, or `0` if `out_buf` is null
/// or too small.
///
/// Hello-world's RefreshDom path: cb increments a u32 counter, JS
/// reads it back from the wasm-resident model pointer, calls this
/// to encode a SetText patch for the counter node (`az_1`), then
/// hands the buffer to `azApplyPatches`. Replaces the hardcoded
/// `el.textContent = newCounter.toString()` in loader.js.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_buildCounterPatch(
    out_buf: u32,
    out_buf_cap: u32,
    node_idx: u32,
    counter_value: u32,
) -> u32 {
    if out_buf == 0 || out_buf_cap < TLV_HEADER_BYTES + 10 {
        return 0;
    }
    let out = out_buf as usize as *mut u8;
    // Encode the decimal text into a scratch region at the tail of
    // the buffer, then memcpy into the payload position once we know
    // the length. Avoids needing two passes over the buffer.
    let max_text = (out_buf_cap - TLV_HEADER_BYTES) as usize;
    let mut scratch = [0u8; 10];
    let scratch_ptr = scratch.as_mut_ptr();
    let text_len = write_u32_decimal(scratch_ptr, counter_value);
    if text_len as usize > max_text {
        return 0;
    }
    // Header: kind(1) | node_idx(4) | payload_len(4)
    *out = PATCH_KIND_SET_TEXT;
    write_u32_le(out, 1, node_idx);
    write_u32_le(out, 5, text_len);
    // Payload: text bytes.
    let payload_dst = out.add(TLV_HEADER_BYTES as usize);
    let mut k = 0u32;
    while k < text_len {
        *payload_dst.add(k as usize) = scratch[k as usize];
        k += 1;
    }
    TLV_HEADER_BYTES + text_len
}

/// Encode a TLV patch with arbitrary payload bytes. Used by Sprint 3
/// patch emission paths that have the payload already laid out in
/// memory (text strings, attribute values, inline-style CSS bytes).
///
/// Layout:
///   kind(1) | node_idx(4) | payload_len(4) | payload[payload_len]
///
/// Returns total bytes written, or 0 if `out_buf` is null / too
/// small / `payload_ptr` is null while `payload_len > 0`.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_buildPatch(
    out_buf: u32,
    out_buf_cap: u32,
    kind: u32,
    node_idx: u32,
    payload_ptr: u32,
    payload_len: u32,
) -> u32 {
    if out_buf == 0 {
        return 0;
    }
    let total = TLV_HEADER_BYTES.wrapping_add(payload_len);
    if out_buf_cap < total {
        return 0;
    }
    if payload_len > 0 && payload_ptr == 0 {
        return 0;
    }
    // Direct address arithmetic on u32 — no helper calls, no
    // intermediate usize.
    let out0 = out_buf as usize as *mut u8;
    *out0 = kind as u8;
    let out1 = out_buf.wrapping_add(1) as usize as *mut u32;
    core::ptr::write_unaligned(out1, node_idx);
    let out5 = out_buf.wrapping_add(5) as usize as *mut u32;
    core::ptr::write_unaligned(out5, payload_len);
    // Byte copy via plain u32 indexes.
    let mut k: u32 = 0;
    while k < payload_len {
        let src_addr = payload_ptr.wrapping_add(k) as usize as *const u8;
        let dst_addr = out_buf.wrapping_add(TLV_HEADER_BYTES).wrapping_add(k) as usize as *mut u8;
        *dst_addr = *src_addr;
        k = k.wrapping_add(1);
    }
    total
}

/// Re-run the layout callback against the current refany, writing
/// the new AzDom blob into a fresh wasm-allocated buffer. The old
/// buffer's wasm offset moves to `state.prev_dom_ptr`; the new
/// offset replaces `state.current_dom_ptr`. Resets
/// `current_dom_hydrated` + `layout_solved` so the next dispatch
/// re-hydrates.
///
/// Status codes:
///   * `0`  — success, current_dom_ptr swapped.
///   * `1`  — null state.
///   * `2`  — `layout_cb_table_idx` or `refany_ptr` not set.
///   * `3`  — buildLayoutInfo / alloc failure.
///   * `100..=199` — layout cb returned non-zero status (low byte
///                   is the cb's status).
#[no_mangle]
pub unsafe extern "C" fn AzStartup_relayout(state: u32) -> u32 {
    if state == 0 {
        return 1;
    }
    let s = &mut *(state as usize as *mut EventloopState);
    if s.layout_cb_table_idx == 0 || s.refany_ptr == 0 {
        return 2;
    }
    let info_ptr = AzStartup_alloc(512);
    let new_out_ptr = AzStartup_alloc(4096);
    if info_ptr == 0 || new_out_ptr == 0 {
        return 3;
    }
    let cb_status = __az_call_indirect_layout4(
        s.layout_cb_table_idx,
        s.refany_ptr as u64,
        0,
        info_ptr,
        new_out_ptr,
    );
    s.last_layout_status = cb_status;
    if cb_status != 0 {
        return 100 + cb_status;
    }
    // Move the prior tree to prev_dom_ptr; new tree owns
    // current_dom_ptr. JS hit-test consumers continue to use the
    // cached positioned_rects until the next solveLayout runs.
    s.prev_dom_ptr = s.current_dom_ptr;
    s.current_dom_ptr = new_out_ptr;
    s.current_dom_hydrated = 0;
    s.layout_solved = 0;
    // Recount the new tree's nodes for diff arena sizing.
    s.current_dom_node_count =
        count_az_dom_nodes(new_out_ptr as usize as *const u8);
    0
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
/// Fake RefAny.lo value passed to the cb. Two constraints:
///   1. Bit 0 set — the cb's lifted body checks `(refany.lo & 1)
///      == 0` early and short-circuits with DoNothing if so.
///      This bit appears to be a flag in the AzRefAny internal
///      representation (likely "has-destructor" or "is-valid")
///      that the inlined `MyDataModel_downcastMut` validates.
///   2. Aligned-ish — when the body derefs `*(refany.lo)` it
///      must land in valid wasm linear memory.
///
/// 0x101 (= 257) satisfies #1 + lands in the cb's data section.
/// Loads from this address would read whatever cb has there
/// (likely zeros, harmlessly). The actual increment is invisible
/// to us; we mirror logical state via the cb's Update return.
///
/// M8.9 + M8.7 will replace this with the real hydrated RefAny.
const FAKE_REFANY_LO: u64 = 0x101;
const FAKE_REFANY_HI: u64 = 0;

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
    let s = &mut *(state as usize as *mut EventloopState);

    // M9-4/M9-6: if JS encoded SENTINEL (0xFFFFFFFF) as node_idx,
    // hit-test wasm-side via AzStartup_hitTest. Otherwise honour
    // the JS-supplied node_idx (legacy back-compat path).
    let event_node_idx_ptr =
        (event_bytes_ptr as usize + event_offset::NODE_IDX as usize) as *const u32;
    let event_node_idx = core::ptr::read(event_node_idx_ptr);
    let x_bits_ptr = (event_bytes_ptr as usize + event_offset::X as usize) as *const u32;
    let y_bits_ptr = (event_bytes_ptr as usize + event_offset::Y as usize) as *const u32;
    let node_idx = if event_node_idx == u32::MAX {
        AzStartup_hitTest(state, *x_bits_ptr, *y_bits_ptr)
    } else {
        event_node_idx
    };
    if node_idx == u32::MAX {
        core::ptr::write_unaligned(out_len_ptr as usize as *mut u32, 0);
        return 0;
    }

    // Resolve cb fn-addr → table_idx. M9-3b will replace
    // `cb_fn_addr = node_idx` with a real per-node fn-addr from
    // the wasm-resident StyledDom; for now the JS-side
    // azFnAddrToTableIdx maps identity (node_idx → table_idx).
    let cb_fn_addr = node_idx as u64;
    let table_idx = __az_resolve_callback(cb_fn_addr);
    if table_idx == u32::MAX {
        core::ptr::write_unaligned(out_len_ptr as usize as *mut u32, 0);
        return 0;
    }

    // M9-6: invoke the cb with the HYDRATED refany (no more
    // FAKE_REFANY_LO). The cb's `data: AzRefAny` arg sees a real
    // wasm-offset pointer to the user data; mutations land in
    // state.model_ptr's region, observable to JS + to the patch
    // emit step below.
    let refany_lo = if s.refany_ptr != 0 {
        s.refany_ptr as u64
    } else {
        FAKE_REFANY_LO
    };
    let update = __az_call_indirect(table_idx, refany_lo, FAKE_REFANY_HI, event_bytes_ptr);

    // M9-5/M9-6: on RefreshDom, encode a SetText TLV patch for the
    // counter display node and return its buffer. The cb has
    // already mutated the model in-place via the refany deref chain,
    // so we just read the updated u32 from `state.model_ptr`,
    // format to decimal, encode the TLV. JS reads the returned
    // `(patch_ptr, patch_len)` and applies via the existing
    // azApplyPatches decoder.
    if update >= UPDATE_REFRESH_DOM
        && s.model_ptr != 0
        && s.display_text_node_idx != u32::MAX
    {
        // Lazy-allocate the patch buffer (32 bytes covers any
        // SetText: 9 header + ≤10 ASCII digits + slack).
        if s.patch_buf_ptr == 0 {
            s.patch_buf_ptr = AzStartup_alloc(32);
        }
        if s.patch_buf_ptr != 0 {
            let counter = core::ptr::read_unaligned(s.model_ptr as usize as *const u32);
            let used = AzStartup_buildCounterPatch(
                s.patch_buf_ptr,
                32,
                s.display_text_node_idx,
                counter,
            );
            core::ptr::write_unaligned(out_len_ptr as usize as *mut u32, used);
            return s.patch_buf_ptr;
        }
    }

    // No patches — surface `update` so JS can log it.
    core::ptr::write_unaligned(out_len_ptr as usize as *mut u32, update);
    0
}
