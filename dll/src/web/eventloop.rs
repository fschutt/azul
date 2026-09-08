//! Eventloop / HeadlessWindow-simulator surface.
//!
//! Defines the `AzStartup_*` C-ABI functions that the remill pipeline
//! lifts into `azul-mini.wasm` at server startup. The lifted module is
//! what JS calls to drive the browser-side event loop.
//!
//! # Model
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
use azul_css::css::Css;
use azul_css::AzString;

/// WEB-LIFT FIX : the embedded fallback font, exposed at MODULE level so the
/// transpiler can force-mirror its FULL byte range into the wasm. The lift otherwise only
/// mirrors statically-accessed pages (`collect_synth_data_pages`), and this 226 KiB const is
/// read by DYNAMIC index — so only its ~first 28 bytes get mirrored and every table read past
/// the header returns 0 (proven: font_bytes[124]/[93596] read 0 in the wasm). Both the
/// real load (`with_memory_fonts(... .to_vec())`) and any direct parse see the truncated const.
pub(crate) const AZ_WEB_FALLBACK_FONT_BYTES: &[u8] =
    include_bytes!("../../../examples/assets/fonts/SourceSerifPro-Regular.ttf");

/// Native `(address, len)` of [`AZ_WEB_FALLBACK_FONT_BYTES`] in THIS process. The transpiler
/// runs in-process (web-transpiler-static) and calls this to add the font's pages to the
/// mirror set so the full TTF lands in wasm linear memory at the matching synth offset.
pub(crate) fn az_web_fallback_font_native() -> (usize, usize) {
    (
        AZ_WEB_FALLBACK_FONT_BYTES.as_ptr() as usize,
        AZ_WEB_FALLBACK_FONT_BYTES.len(),
    )
}

// WEB-FONT-VIA-JS : the embedded font const can't be reliably mirrored into the
// lifted wasm — it's read by dynamic index so only its header lands, and force-mirroring it
// lands at a synth base that differs from where the lifted code reads (a deep lift-internals
// mismatch). The robust fix (how a real web app loads fonts): the JS harness allocates a wasm
// buffer (AzStartup_alloc), writes the TTF bytes into wasm linear memory, and registers it via
// AzStartup_setFallbackFont. Those bytes are RUNTIME data in wasm memory — no const, no mirror,
// no embedded-data synth mapping. WEB_FONT_PTR/LEN are plain scalar statics (reliable across
// the lift). `web_fallback_font_bytes()` returns the JS buffer if set, else the const.
static mut WEB_FONT_PTR: usize = 0;
static mut WEB_FONT_LEN: usize = 0;

/// Register a fallback font buffer that the JS harness has written into wasm linear memory.
/// `ptr` is a wasm memory offset (e.g. from [`AzStartup_alloc`]); `len` is the byte count.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_setFallbackFont(ptr: u32, len: u32) {
    WEB_FONT_PTR = ptr as usize;
    WEB_FONT_LEN = len as usize;
}

/// Bytes the web layout should use for the universal fallback font: the JS-provided buffer if
/// one was registered via [`AzStartup_setFallbackFont`], otherwise the embedded const (correct
/// natively; only partially mirrored on the lifted web path).
fn web_fallback_font_bytes() -> &'static [u8] {
    unsafe {
        if WEB_FONT_PTR != 0 && WEB_FONT_LEN != 0 {
            core::slice::from_raw_parts(WEB_FONT_PTR as *const u8, WEB_FONT_LEN)
        } else {
            AZ_WEB_FALLBACK_FONT_BYTES
        }
    }
}

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
    pub const CLICK: u32 = 0;
    pub const MOUSEDOWN: u32 = 1;
    pub const MOUSEUP: u32 = 2;
    pub const MOUSEMOVE: u32 = 3;
    pub const DBLCLICK: u32 = 4;
    pub const WHEEL: u32 = 5;
    pub const KEYDOWN: u32 = 6;
    pub const KEYUP: u32 = 7;
    pub const FOCUSIN: u32 = 8;
    pub const FOCUSOUT: u32 = 9;
    pub const RESIZE: u32 = 10;
    pub const SCROLL: u32 = 11;
    // S1 (2026-06-11) — non-bubbling pointer events routed by DOM target,
    // plus right-click. Mirrors azul HoverEventFilter::MouseEnter /
    // MouseLeave / RightMouseUp.
    pub const MOUSEENTER: u32 = 12;
    pub const MOUSELEAVE: u32 = 13;
    pub const CONTEXTMENU: u32 = 14;
    /// W3C `mouseover` — the pointer ENTERED the node, bubbling.
    ///
    /// Distinct from `MOUSEMOVE` since the MouseOver/MouseMove split (G2-a):
    /// before it, azul's `MouseOver` carried movement semantics and the loader
    /// decoded a browser `mouseover` as `MOUSEMOVE`. Once `MouseOver` became
    /// the ENTRY event, that left both filters registering the same kind, so
    /// they were indistinguishable and a `MouseOver` subscriber still fired on
    /// every move. APPENDED, because the value is a wire code shared with
    /// `loader_js.rs`.
    pub const MOUSEOVER: u32 = 15;
    /// 9d-i-a: RELATIVE mouse motion while the pointer is locked - the web
    /// shell's `RawMouseMotion`. The `X`/`Y` slots carry `movementX` /
    /// `movementY` as 24.8 fixed-point `i32` (see [`super::fixed_point`]) and
    /// `BUTTON_OR_KEY` carries the `pointerId` as the device. Window-scoped,
    /// so it BROADCASTS to every node registered for it, and it is dropped
    /// unless [`super::EventloopState::is_cursor_locked`] is set - the same
    /// gate X11's `handle_xi_raw_motion` applies. APPENDED (wire code).
    pub const RAWMOUSEMOTION: u32 = 16;
    /// 9d-i-a: `document`'s `pointerlockchange` - the web shell's
    /// `PointerLockChange`. `BUTTON_OR_KEY` is `1` when the lock is now
    /// held and `0` when it was released (by `exitPointerLock`, Escape, or
    /// the browser taking it away on focus loss). Keeps
    /// [`super::EventloopState::is_cursor_locked`] truthful and broadcasts.
    /// APPENDED (wire code).
    pub const POINTERLOCKCHANGE: u32 = 17;
}

/// Common event-bytes layout offsets. JS writes these with
/// `DataView.setUint32/Float32(off, val, /*LE=*/ true)`. Per-kind
/// extras live past `MODIFIERS`.
pub mod event_offset {
    pub const NODE_IDX: u32 = 0;
    pub const X: u32 = 4;
    pub const Y: u32 = 8;
    pub const BUTTON_OR_KEY: u32 = 12;
    pub const MODIFIERS: u32 = 16;
    /// `u32` byte length of the per-kind tail that starts at [`PAYLOAD`].
    /// Every encoder writes it - the buffer comes from a bump allocator and
    /// is NOT zeroed, so a slot left unwritten would read a previous
    /// dispatch's length.
    pub const PAYLOAD_LEN: u32 = 20;
    /// First byte of the per-kind tail.
    pub const PAYLOAD: u32 = 24;
}

/// 24.8 fixed point, the wire form of every sub-pixel coordinate the loader
/// sends: `Math.round(v * 256)` as an `i32`.
///
/// The header's `X`/`Y` are integer pixels because `f32::from_bits` proved
/// unreliable through the remill lift (see `azDispatch`). Integer pixels are
/// enough for hit-testing but not for the two things the tail carries: a
/// coalesced stroke, where the sub-pixel positions ARE the point, and raw
/// motion, where a slow mouse reports fractions of a pixel per sample and
/// rounding each one to 0 would freeze it. Fixed point keeps both without
/// relying on a float bit-cast in lifted code: the decode is one integer
/// load and one `f32` division.
pub mod fixed_point {
    pub const SCALE: f32 = 256.0;

    #[must_use]
    pub fn to_f32(v: i32) -> f32 {
        v as f32 / SCALE
    }

    /// The loader's encoder, natively: what `Math.round(v * 256)` produces.
    #[must_use]
    pub fn from_f32(v: f32) -> i32 {
        (v * SCALE).round() as i32
    }
}

/// 10e-i: the coalesced / predicted pointer samples a `pointermove`,
/// `pointerdown` or `pointerup` carries in its tail.
///
/// This is the web mirror of `TouchState::coalesced_points` /
/// `predicted_points`, which iOS fills from `coalescedTouches` /
/// `predictedTouches` and which `CallbackInfo::get_coalesced_touches` /
/// `get_predicted_touches` read. Same contract: coalesced samples are the
/// ones the browser captured BETWEEN the previous dispatch and this one,
/// oldest first, WITHOUT the dispatched event's own position; predicted
/// samples are extrapolations, oldest first, never to be persisted.
///
/// Tail layout (all little-endian, at [`event_offset::PAYLOAD`]):
///
/// ```text
/// n_coalesced:u32 | n_predicted:u32 | (x:i32, y:i32) * n_coalesced | (x:i32, y:i32) * n_predicted
/// ```
///
/// with every coordinate in [`fixed_point`]. Capacities are fixed by the
/// 256-byte event buffer: `24 + 8 + 8 * (24 + 4) = 256` exactly. The loader
/// keeps the NEWEST coalesced samples when there are more (a stroke's most
/// recent segment is the one still being drawn) and the FIRST predicted
/// ones (a prediction gets less trustworthy the further out it reaches).
pub mod pointer_samples {
    use super::{event_offset, fixed_point, EVENT_BYTES_LEN};

    /// `n_coalesced | n_predicted` header, in bytes.
    pub const HEADER_BYTES: u32 = 8;
    /// `(x, y)` pair, in bytes.
    pub const SAMPLE_BYTES: u32 = 8;
    pub const MAX_COALESCED: usize = 24;
    pub const MAX_PREDICTED: usize = 4;

    const _FITS: () = assert!(
        event_offset::PAYLOAD + HEADER_BYTES + SAMPLE_BYTES * (MAX_COALESCED + MAX_PREDICTED) as u32
            <= EVENT_BYTES_LEN,
        "pointer sample capacities must fit the 256-byte event buffer"
    );

    /// Fixed-capacity, plain-old-data store: no allocation on the dispatch
    /// path and nothing for the lift to get wrong.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct PointerSampleBuffer {
        pub coalesced: [[f32; 2]; MAX_COALESCED],
        pub coalesced_len: u32,
        pub predicted: [[f32; 2]; MAX_PREDICTED],
        pub predicted_len: u32,
    }

    impl PointerSampleBuffer {
        pub const EMPTY: Self = Self {
            coalesced: [[0.0; 2]; MAX_COALESCED],
            coalesced_len: 0,
            predicted: [[0.0; 2]; MAX_PREDICTED],
            predicted_len: 0,
        };

        pub fn clear(&mut self) {
            *self = Self::EMPTY;
        }

        /// The coalesced samples, oldest first.
        #[must_use]
        pub fn coalesced(&self) -> &[[f32; 2]] {
            &self.coalesced[..(self.coalesced_len as usize).min(MAX_COALESCED)]
        }

        /// The predicted samples, oldest first.
        #[must_use]
        pub fn predicted(&self) -> &[[f32; 2]] {
            &self.predicted[..(self.predicted_len as usize).min(MAX_PREDICTED)]
        }
    }

    fn read_u32(tail: &[u8], at: usize) -> Option<u32> {
        let b = tail.get(at..at + 4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Decode a tail into `out`. A tail that is too short for what its own
    /// counts claim, or that claims more than the capacities, leaves `out`
    /// EMPTY rather than half-filled: a truncated stroke is a lie about
    /// where the pointer was. An empty tail (the mouse-event fallback, a
    /// browser without `getCoalescedEvents`) decodes to empty, which is
    /// also what "nothing moved between frames" looks like.
    pub fn decode(tail: &[u8], out: &mut PointerSampleBuffer) {
        out.clear();
        if tail.is_empty() {
            return;
        }
        let (Some(n_c), Some(n_p)) = (read_u32(tail, 0), read_u32(tail, 4)) else {
            return;
        };
        let (n_c, n_p) = (n_c as usize, n_p as usize);
        if n_c > MAX_COALESCED || n_p > MAX_PREDICTED {
            return;
        }
        let needed = HEADER_BYTES as usize + SAMPLE_BYTES as usize * (n_c + n_p);
        if tail.len() < needed {
            return;
        }
        let mut at = HEADER_BYTES as usize;
        for i in 0..n_c {
            let x = read_u32(tail, at).unwrap_or(0) as i32;
            let y = read_u32(tail, at + 4).unwrap_or(0) as i32;
            out.coalesced[i] = [fixed_point::to_f32(x), fixed_point::to_f32(y)];
            at += SAMPLE_BYTES as usize;
        }
        for i in 0..n_p {
            let x = read_u32(tail, at).unwrap_or(0) as i32;
            let y = read_u32(tail, at + 4).unwrap_or(0) as i32;
            out.predicted[i] = [fixed_point::to_f32(x), fixed_point::to_f32(y)];
            at += SAMPLE_BYTES as usize;
        }
        out.coalesced_len = n_c as u32;
        out.predicted_len = n_p as u32;
    }

    /// The loader's `azEncodePointerSamples`, natively - exists so the
    /// decoder can be tested against the exact byte layout the JS writes.
    #[must_use]
    pub fn encode(coalesced: &[[f32; 2]], predicted: &[[f32; 2]]) -> Vec<u8> {
        // Newest coalesced, first predicted: the same truncation the loader does.
        let c_start = coalesced.len().saturating_sub(MAX_COALESCED);
        let coalesced = &coalesced[c_start..];
        let predicted = &predicted[..predicted.len().min(MAX_PREDICTED)];
        let mut out = Vec::with_capacity(
            HEADER_BYTES as usize + SAMPLE_BYTES as usize * (coalesced.len() + predicted.len()),
        );
        out.extend_from_slice(&(coalesced.len() as u32).to_le_bytes());
        out.extend_from_slice(&(predicted.len() as u32).to_le_bytes());
        for [x, y] in coalesced.iter().chain(predicted.iter()) {
            out.extend_from_slice(&fixed_point::from_f32(*x).to_le_bytes());
            out.extend_from_slice(&fixed_point::from_f32(*y).to_le_bytes());
        }
        out
    }
}

/// 9d-i-a: raw motion accumulated for the dispatch in flight - the web
/// mirror of `DeviceEventManager::note_raw_motion` / `take_raw_motion`.
/// Set while a `RAWMOUSEMOTION` broadcast runs (what a callback would read
/// through `get_raw_mouse_motion`) and drained right after, like every
/// other per-pass pending value in the desktop shell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawMotionAccumulator {
    pub dx: f32,
    pub dy: f32,
    /// The `pointerId` of the pointer that moved.
    pub device_id: u32,
    /// `1` while a delta is pending, `0` after the drain.
    pub pending: u32,
}

impl RawMotionAccumulator {
    pub const NONE: Self = Self {
        dx: 0.0,
        dy: 0.0,
        device_id: 0,
        pending: 0,
    };

    /// Accumulate, like `note_raw_motion`: several deltas between passes add up.
    pub fn note(&mut self, dx: f32, dy: f32, device_id: u32) {
        self.dx += dx;
        self.dy += dy;
        self.device_id = device_id;
        self.pending = 1;
    }

    /// Drain, like `take_raw_motion`.
    pub fn take(&mut self) -> Option<(f32, f32, u32)> {
        if self.pending == 0 {
            return None;
        }
        let out = (self.dx, self.dy, self.device_id);
        *self = Self::NONE;
        Some(out)
    }
}

// =====================================================================
// AzUpdate values (mirror dll_api_internal.rs's enum).
// =====================================================================

pub const UPDATE_DO_NOTHING: u32 = 0;
pub const UPDATE_REFRESH_DOM: u32 = 1;
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
    /// (per-EventFilter dispatch): event KIND each cb node is
    /// registered for, indexed by node_idx (the loader derives the kind from
    /// the emitted `data-az-ev` attribute via AzStartup_registerCbNodeKind).
    /// 0xFF = no registration → dispatchEvent keeps the legacy
    /// invoke-on-any-kind behavior for that node.
    pub cb_node_kinds: [u8; 64],

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
    /// transitive deps don't survive the web-lift today (the cascade
    /// is the highest-risk part of the backend to lift). For Sprint 1 we
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

    // S1 input-event fields ──────────────────────────

    /// node_idx of the currently focused node (`u32::MAX` = none).
    /// Updated by FOCUSIN/FOCUSOUT dispatches; KEYDOWN/KEYUP route
    /// here when set, else broadcast to kind-registered nodes.
    pub focused_node_idx: u32,
    /// Most recent viewport dimensions from a RESIZE dispatch
    /// (CSS pixels). 0 until the first resize. Later slices expose
    /// these through CallbackInfo's window state.
    pub viewport_w: u32,
    pub viewport_h: u32,

    // 10e-i / 9d-i-a input-state fields (2026-09-04) ─────────────
    /// Web mirror of `TouchState::coalesced_points` / `predicted_points`:
    /// the samples the most recent pointer dispatch carried in its tail
    /// (see [`pointer_samples`]). Rewritten on every `MOUSEDOWN` /
    /// `MOUSEMOVE` / `MOUSEUP`, cleared after the up - the iOS rule.
    pub pointer_samples: pointer_samples::PointerSampleBuffer,
    /// Web mirror of `mouse_state.is_cursor_locked`. `1` between a
    /// `POINTERLOCKCHANGE` that reported the lock held and one that
    /// reported it released. It is what the BROWSER says, never what was
    /// asked for: `requestPointerLock` can be refused (no user gesture,
    /// sandboxed frame) and the browser drops the lock on its own (Escape,
    /// tab switch), and `RAWMOUSEMOTION` is gated on this flag.
    pub is_cursor_locked: u32,
    /// Web mirror of `DeviceEventManager`'s pending raw motion: the delta
    /// of the `RAWMOUSEMOTION` broadcast in flight, drained afterwards.
    pub raw_motion: RawMotionAccumulator,
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
    // `from_size_align_unchecked` (align=8 is always valid): the CHECKED `from_size_align` calls
    // `Layout::is_size_align_valid`, a tiny core::alloc fn that the web lift Leaf-stubs (returns
    // X0=0 = false) → `from_size_align` always Err → this allocator returned 0 → empty DOM.
    let layout = unsafe { Layout::from_size_align_unchecked(size as usize, 8) };
    let ptr = unsafe { alloc(layout) };
    ptr as usize as u32
}

/// Free a buffer previously returned by [`AzStartup_alloc`].
#[no_mangle]
pub extern "C" fn AzStartup_free(ptr: u32, size: u32) {
    if ptr == 0 || size == 0 {
        return;
    }
    // unchecked: matches AzStartup_alloc — is_size_align_valid is Leaf-stubbed in the web lift.
    let layout = unsafe { Layout::from_size_align_unchecked(size as usize, 8) };
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
        cb_node_kinds: [0xFF; 64],
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
        focused_node_idx: u32::MAX,
        viewport_w: 0,
        viewport_h: 0,
        pointer_samples: pointer_samples::PointerSampleBuffer::EMPTY,
        is_cursor_locked: 0,
        raw_motion: RawMotionAccumulator::NONE,
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

/// Rebuild the app state from its JSON reflection instead of a raw byte
/// image, for models whose bytes are meaningless in the guest.
///
/// `AzStartup_hydrate` copies the model's native byte image verbatim. That
/// is right for plain-old-data state (hello-world's `{ counter: u32 }`) and
/// wrong for anything holding pointers: a `String`'s heap address belongs to
/// the SERVER process, so the first deref in the guest leaves linear memory.
///
/// The app registers its `extern "C" fn(Json) -> ResultRefAnyString` via
/// `set_deserialize_fn` in `main()`, which never runs here — so the loader
/// passes the address (already synth-translated by the server) to
/// [`AzStartup_registerStateDeserializer`], and this fn calls it through the
/// stored pointer. In the lifted world that indirect call goes through the
/// remill dispatcher, so the deserializer's body must be part of this
/// bundle; the mini lift seeds it as an extra root.
///
/// `json_ptr`/`json_len` describe UTF-8 JSON text in guest memory. Returns
/// the wasm offset of the new `AzRefAny`, or `0` on any failure — never a
/// silent fallback to the raw-bytes path, which would resurrect exactly the
/// corruption this exists to avoid.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_hydrateJson(
    state: u32,
    json_ptr: u32,
    json_len: u32,
) -> u32 {
    if state == 0 || json_ptr == 0 || json_len == 0 {
        return 0;
    }
    let s = &mut *(state as usize as *mut EventloopState);
    let deser_fn = s.state_deserializer as usize;
    if deser_fn == 0 {
        return 0;
    }
    let text = core::slice::from_raw_parts(json_ptr as usize as *const u8, json_len as usize);
    let json = match azul_core::json::Json::parse_bytes(text) {
        Ok(j) => j,
        Err(_) => return 0,
    };
    let refany = match azul_layout::json::json_deserialize_to_refany(json, deser_fn) {
        azul_layout::json::ResultRefAnyString::Ok(r) => r,
        azul_layout::json::ResultRefAnyString::Err(_) => return 0,
    };
    let boxed = Box::new(refany);
    let ptr = Box::into_raw(boxed) as usize as u32;
    s.refany_ptr = ptr;
    ptr
}

/// Build a wasm-side `RefAny` from a raw type_id + data buffer.
///
/// Called by loader.js at bootstrap (after `AzStartup_init`): JS allocates
/// `data_size` bytes via [`AzStartup_alloc`], writes the model's bytes, and
/// gets back the wasm offset of a fully constructed `AzRefAny`. All
/// allocations route through the bump allocator, so every deref the lifted
/// callback does lands in linear memory at native-layout positions.
/// `run_destructor` is `false`: the wasm instance never tears the hydrated
/// RefAny down. `type_id` is split into two u32 halves so JS avoids BigInt
/// args (and the lift wrapper needs no 64-bit param slot).
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
pub unsafe extern "C" fn AzStartup_registerStateDeserializer(state: u32, fn_addr: u64) {
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
    let cb_status = __az_call_indirect_layout4(table_idx, refany_ptr as u64, 0, info_ptr, out_ptr);
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

/// (per-EventFilter dispatch): like [`AzStartup_registerCbNode`]
/// but also records the EVENT KIND (the loader's EVT_* int, derived from the
/// `data-az-ev` attribute that mirrors the callback's registered EventFilter).
/// [`AzStartup_dispatchEvent`] only invokes the hit node's callback when the
/// incoming kind matches — a single physical click no longer triple-fires
/// through mousedown + mouseup + click.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_registerCbNodeKind(state: u32, node_idx: u32, kind: u32) {
    if state == 0 {
        return;
    }
    let s = &mut *(state as usize as *mut EventloopState);
    s.last_registered_cb_node_idx = node_idx;
    s.cb_fn_cache.insert(node_idx, node_idx as u64);
    if (node_idx as usize) < s.cb_node_kinds.len() {
        s.cb_node_kinds[node_idx as usize] = kind as u8;
    }
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
pub unsafe extern "C" fn AzStartup_hitTest(state: u32, x_f32_bits: u32, y_f32_bits: u32) -> u32 {
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
        // Skip sentinel/unpositioned rects (display:none, anonymous, or
        // not-yet-laid-out nodes carry u32::MAX coords).
        if rx == u32::MAX {
            continue;
        }
        if x >= rx && x < rx.wrapping_add(rw) && y >= ry && y < ry.wrapping_add(rh) {
            return i;
        }
    }
    // NO rect matched → return MAX (genuine miss). The old
    // `last_registered_cb_node_idx` fallback made EVERY click in empty space
    // dispatch to the most-recently-registered callback (the button) — i.e.
    // "click anywhere increments the counter". A real bbox miss must be a miss;
    // dispatchEvent then does nothing. (The `positioned_rects_len == 0` guard
    // above still covers the "layout never ran" case for trivial demos.)
    u32::MAX
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
// **Why a marker field instead of `Option<StyledDom>` here**:
// building a real `StyledDom` requires running the cascade — that
// path's transitive lift complexity is the highest-risk part of the
// web backend. For Sprint 1 we use the AzDom blob as the
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
        let child_ptr_raw = core::ptr::read_unaligned(dvec as *const usize) as *const u8;
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
    s.current_dom_node_count = count;
    s.current_dom_hydrated = 1;

    // Run the cascade on the layout-cb's real Dom (`dom_ref`, the AzDom
    // the lifted layout fn deposited via the hidden sret return). Same
    // `StyledDom::create` the desktop App.run uses; with an empty Css the
    // selector pass is a no-op, but UA CSS + inheritance still run so
    // widget defaults (font-size, padding) apply.
    let styled = StyledDom::create(dom_ref, Css::empty());
    let boxed = Box::new(styled);
    let ptr_val = Box::into_raw(boxed) as usize as u32;
    finalize_hydrate(state, ptr_val);
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

// (Removed AzStartup_writeStructConsts: adding it to the dylib deterministically
// re-codegened the lifted cascade into an OOB-trapping shape. The JS harness now
// HARDCODES the struct layout consts (size_of(Dom)=240, offset_of(Dom,children)=152,
// size_of(NodeData)=152, offset_of(NodeData,node_type)=0, offset_of(StyledDom,node_data)=48),
// extracted natively via a one-off `core::mem::offset_of!` test. The structs have no
// cfg-gated fields, so the layout is build-independent.)

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
/// many direct children get auto-wrapped as `VirtualView`.
pub const AZ_AUTO_VIRTUALIZE_THRESHOLD: u32 = 500;

/// Set the auto-virtualize threshold. `0` disables. Defaults to
/// [`AZ_AUTO_VIRTUALIZE_THRESHOLD`].
#[no_mangle]
pub unsafe extern "C" fn AzStartup_setAutoVirtualizeThreshold(state: u32, threshold: u32) {
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
pub unsafe extern "C" fn AzStartup_setVirtualViewProvider(state: u32, table_idx: u32) {
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
        core::ptr::write_unaligned(p, 0); // x
        core::ptr::write_unaligned(p.add(1), i * h); // y
        core::ptr::write_unaligned(p.add(2), viewport_w); // w
        core::ptr::write_unaligned(p.add(3), h); // h
        i += 1;
    }
    s.positioned_rects_ptr = buf;
    s.positioned_rects_len = node_count;
    s.layout_solved = 1;
    0
}

// =====================================================================
// M12.7 — REAL layout solver (replaces the block-flow stub above)
// =====================================================================
//
// `AzStartup_solveLayoutReal` is the lifted entry into the actual
// `azul-layout` solver. It consumes the cascaded `StyledDom` produced
// by `AzStartup_hydrateStyledDom` (`current_dom_styled_ptr`) + a
// viewport, runs `LayoutWindow::layout_and_generate_display_list`
// (→ `layout_dom_recursive` → `solver3::layout_document` → taffy
// block/flex/grid), and writes the per-node positioned rects into
// `state.positioned_rects_ptr` in the SAME 16-byte (x,y,w,h u32)
// layout the hit-test reads. This mirrors the desktop core loop
// (see `dll/src/web/server.rs::dispatch_callback` +
// `layout/tests/flexbox_integration.rs`), minus the display-list
// consumption and CPU renderer.
//
// The function is registered in `EVENTLOOP_SYMBOLS` (mod.rs) + given a
// `CallbackSignature` (transpiler_remill.rs) so the transitive lift
// pipeline pulls in the real solver's dependency graph. Whatever the
// `bl`-walk reaches gets lifted; unreached paths (a11y is cfg'd out,
// most GPU/webrender transaction code is off the box-solving path)
// never enter the wasm.

/// **M12.7 — REAL layout solve.** See the section comment above.
///
/// Consumes the cascaded `StyledDom` at `current_dom_styled_ptr`
/// (zeroing the slot, since layout takes ownership) and fills
/// `positioned_rects_ptr` with one `(x, y, w, h)` u32 quad per DOM
/// node, indexed by node_idx — exactly what `AzStartup_hitTest`
/// walks.
///
/// Status codes:
///   * `0` — success.
///   * `1` — null state pointer.
///   * `2` — `current_dom_styled_ptr` is 0 (hydrate didn't run).
///   * `3` — StyledDom has 0 nodes.
///   * `4` — `LayoutWindow::new` failed.
///   * `5` — `layout_and_generate_display_list` returned `Err`.
///   * `6` — allocator failure.
#[no_mangle]
pub unsafe extern "C" fn AzStartup_solveLayoutReal(
    state: u32,
    viewport_w: u32,
    viewport_h: u32,
) -> u32 {
    use azul_core::dom::{DomId, DomNodeId};
    use azul_core::geom::LogicalSize;
    use azul_core::id::NodeId;
    use azul_core::resources::RendererResources;
    use azul_core::styled_dom::NodeHierarchyItemId;
    use azul_layout::callbacks::ExternalSystemCallbacks;
    use azul_layout::window::LayoutWindow;
    use azul_layout::window_state::FullWindowState;
    use rust_fontconfig::FcFontCache;

    if state == 0 {
        return 1;
    }
    let s = &mut *(state as usize as *mut EventloopState);
    if s.current_dom_styled_ptr == 0 {
        return 2;
    }

    // Take ownership of the boxed cascaded StyledDom (layout consumes
    // `root_dom` by value). Zero the slot so the diagnostic getters
    // don't read freed memory, and so a second solve is a clean no-op.
    let styled: StyledDom = *Box::from_raw(s.current_dom_styled_ptr as usize as *mut StyledDom);
    s.current_dom_styled_ptr = 0;
    let node_count = styled.node_data.len() as u32;
    if node_count == 0 {
        return 3;
    }

    // Headless layout window. There is no filesystem in wasm, so the disk
    // font loaders (`PathLoader::load_from_path` → `std::fs::read`) all fail —
    // and the solver HARD-ERRORS `LayoutError::Text(FontNotFound)` even for a
    // text-free body. Register one embedded fallback font as an in-MEMORY
    // source (`with_memory_fonts`): it's matched as the universal fallback and
    // loaded via `FontBytes::Owned`, never `std::fs::read`. This both unblocks
    // the bare-body layout AND gives <p>hello</p> real glyph metrics later.
    // WEB-FONT-VIA-JS: prefer the JS-registered buffer (real bytes in wasm memory) over the
    // const (only partially mirrored on the lifted web path). Runtime `let`, not `const`.
    let az_web_fallback_font: &[u8] = web_fallback_font_bytes();
    let fc_cache = FcFontCache::default();
    // rust-fontconfig matches family by SUBSTRING (stored.family.contains(query)).
    // The solver's default FontSelector queries "serif" (text3 default), and DOMs
    // ask for "serif"/"sans-serif"/"monospace". One stored string containing all
    // three generics matches every such query → this one font is the universal
    // fallback. (A specific-name query falls back through its chain to a generic.)
    // Match by BOTH name and family (substring): the default font query is
    // StyleFontFamily::System("serif") (azul_css DEFAULT_FONT_ID), which may
    // populate FcPattern.name OR .family. One string with all generics covers
    // serif/sans-serif/monospace on either field.
    // WEB-LIFT: register the embedded fallback. unicode_ranges MUST be non-empty —
    // `resolve_char` skips fonts whose metadata `unicode_ranges.is_empty()`, so full
    // coverage here makes the last-resort fallback resolve for any char. Build the
    // input Vec via Vec::new()+push (NOT `vec![(...)]`) — the lift DROPS elements of a
    // `vec!` of a complex nested struct (M11/M12 gap), giving with_memory_fonts an
    // EMPTY list → fc_cache.len()=0 → no font → text height 0.
    let mut fc_unicode = Vec::new();
    fc_unicode.push(rust_fontconfig::UnicodeRange {
        start: 0,
        end: 0x10FFFF,
    });
    let fc_pattern = rust_fontconfig::FcPattern {
        name: Some("serif sans-serif monospace".to_string()),
        family: Some("serif sans-serif monospace".to_string()),
        unicode_ranges: fc_unicode,
        ..Default::default()
    };
    let fc_font = rust_fontconfig::FcFont {
        bytes: az_web_fallback_font.to_vec(),
        font_index: 0,
        id: "az_web_fallback".to_string(),
    };
    let mut fc_fonts = Vec::new();
    fc_fonts.push((fc_pattern, fc_font));
    fc_cache.with_memory_fonts(fc_fonts);
    let mut lw = match LayoutWindow::new(fc_cache) {
        Ok(lw) => lw,
        Err(_) => return 4,
    };
    // Web/headless: skip the GPU transform/opacity sync in layout_dom_recursive
    // (display-list-only, no GPU here, and GpuValueCache::synchronize mis-lifts
    // to wasm → OOB). Heap field, not the SKIP_DISPLAY_LIST static (whose
    // store/load is unreliable in the lifted wasm).
    lw.skip_gpu_sync = true;
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(viewport_w as f32, viewport_h as f32);
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = None;

    // Web backend: skip display-list generation. We emit TLV DOM
    // patches, not a display list, so the painter output is dead weight
    // — and skipping it lets the lift drop the entire `display_list`
    // surface (those symbols are classified Leaf in symbol_table.rs, so
    // the transitive walk never descends into the ~300+ painter fns).
    // Positions are computed BEFORE the (now-skipped) display-list step.
    azul_layout::solver3::set_skip_display_list(true);

    // Call the positioning solver directly (not the public
    // `layout_and_generate_display_list`, whose tail does virtual-view
    // scanning + scrollbar GPU registration we don't need on web).
    // Positions land in `layout_cache.calculated_positions`.
    if let Err(_e) = lw.layout_dom_recursive(styled, &ws, &rr, &sc, &mut dbg) {
        // The layout error is EARLY (before block geometry — rects come back
        // all-zero when we fall through), so there's no partial geometry to
        // salvage. Surface a non-zero status so the caller can react.
        return 5;
    }

    // Extract per-DOM-node rects (x, y, w, h) into the positioned-rect
    // cache, indexed by node_idx. `get_node_layout_rect` returns logical
    // (CSS-pixel) coordinates already divided by the hidpi factor.
    let buf_size = node_count.saturating_mul(POSITIONED_RECT_BYTES);
    let buf = AzStartup_alloc(buf_size);
    if buf == 0 {
        return 6;
    }
    // `buf` is alloc-aligned and each node's quad is 4-byte aligned, so a
    // plain `&mut [u32]` (4 lanes per node) is sound — no unaligned writes.
    let rects =
        core::slice::from_raw_parts_mut(buf as usize as *mut u32, (node_count * 4) as usize);
    for i in 0..node_count as usize {
        let node_id = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(i))),
        };
        let lane = &mut rects[i * 4..i * 4 + 4];
        // M12.7 FIX: get_node_layout_rect reads self.layout_cache.tree, but
        // layout_dom_recursive stores the laid-out tree + positions in
        // self.layout_results[dom] (get_node_layout_rect returned None → empty cache).
        // Use get_node_position + get_node_size, which read layout_results via the
        // dom_to_layout mapping (the correct, populated location).
        // Position/size are reported per-axis: a node absent from the layout
        // results writes the 0xFFFFFFFF sentinel (the loader's SENTINEL_NO_NODE)
        // so "not in tree" stays distinguishable from "in tree, sized 0".
        match lw.get_node_position(node_id) {
            Some(p) => {
                lane[0] = p.x.max(0.0).round() as u32;
                lane[1] = p.y.max(0.0).round() as u32;
            }
            None => {
                lane[0] = u32::MAX;
                lane[1] = u32::MAX;
            }
        }
        match lw.get_node_size(node_id) {
            Some(sz) => {
                lane[2] = sz.width.max(0.0).round() as u32;
                lane[3] = sz.height.max(0.0).round() as u32;
            }
            None => {
                lane[2] = u32::MAX;
                lane[3] = u32::MAX;
            }
        }
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
// The per-kind payload layout is the `PATCH_KIND_*` table below; each
// constant's own comment gives the fields that follow its header.

const TLV_HEADER_BYTES: u32 = 1 + 4 + 4;
pub const PATCH_KIND_SET_TEXT: u8 = 1;
pub const PATCH_KIND_SET_ATTR: u8 = 2;
pub const PATCH_KIND_REMOVE_ATTR: u8 = 3;
pub const PATCH_KIND_SET_INLINE_STYLE: u8 = 4;
pub const PATCH_KIND_REMOVE_NODE: u8 = 5;
pub const PATCH_KIND_INSERT_NODE: u8 = 6;
pub const PATCH_KIND_MOVE_NODE: u8 = 7;
pub const PATCH_KIND_REPLACE_SUBTREE: u8 = 8;
pub const PATCH_KIND_FOCUS: u8 = 9;
pub const PATCH_KIND_SCROLL_TO: u8 = 10;
pub const PATCH_KIND_ADD_CLASS: u8 = 11;
pub const PATCH_KIND_REMOVE_CLASS: u8 = 12;
/// 9d-i-a: `CallbackChange::SetPointerLock { locked }` on the web. Payload
/// is one byte, `1` to request the lock and `0` to release it; `node_idx`
/// is ignored (the lock is document-wide: the loader locks the page body).
/// The JS decoder calls `requestPointerLock` / `exitPointerLock`, and the
/// ANSWER comes back as a `POINTERLOCKCHANGE` event - the request is never
/// assumed granted, same as the desktop backends. Like `FOCUS` and
/// `SCROLL_TO`, the decoder exists ahead of its wasm producer: today's
/// callback receives the raw event bytes as its info pointer, so its
/// `set_pointer_lock` has no change list to land in until the wasm-side
/// `CallbackInfo` (S2) arrives.
pub const PATCH_KIND_POINTER_LOCK: u8 = 13;

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
    // THE WEB BUILD'S ONLY CLOCK.
    //
    // `std::time::Instant::now()` panics on wasm32, so `azul_core::task::Instant`
    // answers `Tick(system_tick_now())` there instead. That is only a clock if
    // something advances it — a constant tick would freeze every timer and
    // animation, which is a silent stall and worse than the panic it replaced.
    //
    // A produced DOM patch is exactly one frame: the browser drives redraw and
    // azul hands back a type/length/value patch per frame. So a frame is the
    // tick, which is also precisely what a CSS `t` duration counts.
    //
    // Advanced BEFORE the early returns below on purpose. Those reject a
    // malformed request, not a frame that did not happen, and skipping the
    // increment there would stall time whenever the JS side passed a short
    // buffer.
    azul_core::task::advance_system_tick();

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
    s.current_dom_node_count = count_az_dom_nodes(new_out_ptr as usize as *const u8);
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
/// invokes via [`__az_call_indirect`]. The return value reports the
/// Update enum the user callback produced as a debugging signal;
/// patches themselves go out via `AzStartup_buildCounterPatch`.
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

/// Resolve `node_idx`'s callback to a table index and invoke it with the
/// hydrated refany. The info pointer is still the raw event bytes (S1) —
/// S2 replaces it with a real wasm-side `CallbackInfo`. Returns the cb's
/// `Update`, or 0 when the node has no resolvable callback.
unsafe fn invoke_node_cb(s: &mut EventloopState, node_idx: u32, event_bytes_ptr: u32) -> u32 {
    let table_idx = __az_resolve_callback(node_idx as u64);
    if table_idx == u32::MAX {
        return 0;
    }
    let refany_lo = if s.refany_ptr != 0 {
        s.refany_ptr as u64
    } else {
        FAKE_REFANY_LO
    };
    __az_call_indirect(table_idx, refany_lo, FAKE_REFANY_HI, event_bytes_ptr)
}

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
    // the JS-supplied node_idx (DOM-target events — focusin/out,
    // mouseenter/leave — pass the real target; mouseleave coords lie
    // OUTSIDE the node so hit-testing them would be wrong).
    let event_node_idx_ptr =
        (event_bytes_ptr as usize + event_offset::NODE_IDX as usize) as *const u32;
    let event_node_idx = core::ptr::read(event_node_idx_ptr);
    let x_bits_ptr = (event_bytes_ptr as usize + event_offset::X as usize) as *const u32;
    let y_bits_ptr = (event_bytes_ptr as usize + event_offset::Y as usize) as *const u32;

    // S1 : RESIZE carries (w, h) in the x/y slots — record the
    // viewport so later slices can surface it through CallbackInfo. Window
    // kinds are never hit-tested.
    if _kind == event_kind::RESIZE {
        s.viewport_w = *x_bits_ptr;
        s.viewport_h = *y_bits_ptr;
    }

    let button_or_key =
        *((event_bytes_ptr as usize + event_offset::BUTTON_OR_KEY as usize) as *const u32);

    // 9d-i-a: the lock flag is what the BROWSER reports, applied before any
    // routing so a release and the motion after it can never interleave
    // the wrong way round.
    if _kind == event_kind::POINTERLOCKCHANGE {
        s.is_cursor_locked = u32::from(button_or_key != 0);
    }
    // 9d-i-a: raw motion is dropped unless the lock is held - X11's
    // `handle_xi_raw_motion` gate. The loader gates on its own view of the
    // lock too, but the wasm side holds the flag that mirrors
    // `mouse_state.is_cursor_locked`, so it is the one that decides.
    if _kind == event_kind::RAWMOUSEMOTION {
        if s.is_cursor_locked == 0 {
            core::ptr::write_unaligned(out_len_ptr as usize as *mut u32, 0);
            return 0;
        }
        s.raw_motion.note(
            fixed_point::to_f32(*x_bits_ptr as i32),
            fixed_point::to_f32(*y_bits_ptr as i32),
            button_or_key,
        );
    }

    // 10e-i: the tail of a pointer dispatch carries the coalesced and
    // predicted samples. Bounds come from the caller's `event_bytes_len`,
    // never from the tail's own counts alone.
    let is_pointer_kind = _kind == event_kind::MOUSEDOWN
        || _kind == event_kind::MOUSEMOVE
        || _kind == event_kind::MOUSEUP;
    if is_pointer_kind {
        let tail: &[u8] = if event_bytes_len >= event_offset::PAYLOAD + 4 {
            let payload_len =
                *((event_bytes_ptr as usize + event_offset::PAYLOAD_LEN as usize) as *const u32);
            let avail = event_bytes_len - event_offset::PAYLOAD;
            core::slice::from_raw_parts(
                (event_bytes_ptr as usize + event_offset::PAYLOAD as usize) as *const u8,
                payload_len.min(avail) as usize,
            )
        } else {
            &[]
        };
        pointer_samples::decode(tail, &mut s.pointer_samples);
    }

    // S1 routing:
    //   * RESIZE/SCROLL/KEYDOWN/KEYUP (and the pointer-lock pair) broadcast
    //     to every node whose registered kind matches (azul Window-filter
    //     semantics: fires regardless of pointer position). Focus-filter
    //     keyboard precedence arrives with S2's real CallbackInfo.
    //   * everything else: JS-supplied target (focus/enter/leave events
    //     pass the DOM target) or bbox hit-test on SENTINEL.
    let is_broadcast_kind = _kind == event_kind::RESIZE
        || _kind == event_kind::SCROLL
        || _kind == event_kind::KEYDOWN
        || _kind == event_kind::KEYUP
        || _kind == event_kind::RAWMOUSEMOTION
        || _kind == event_kind::POINTERLOCKCHANGE;
    let node_idx = if is_broadcast_kind {
        u32::MAX
    } else if event_node_idx == u32::MAX {
        AzStartup_hitTest(state, *x_bits_ptr, *y_bits_ptr)
    } else {
        event_node_idx
    };

    // Focus tracking — drives the keyboard routing above.
    if _kind == event_kind::FOCUSIN {
        s.focused_node_idx = node_idx;
    } else if _kind == event_kind::FOCUSOUT && s.focused_node_idx == node_idx {
        s.focused_node_idx = u32::MAX;
    }

    let mut update = 0u32;
    if node_idx != u32::MAX {
        // Single-target path. Per-EventFilter kind check :
        // a node registered for a specific kind only fires on that kind;
        // unregistered nodes (0xFF) keep legacy invoke-on-any-kind.
        let mut kind_ok = true;
        if (node_idx as usize) < s.cb_node_kinds.len() {
            let reg = s.cb_node_kinds[node_idx as usize];
            if reg != 0xFF && reg as u32 != _kind {
                kind_ok = false;
            }
        }
        if kind_ok {
            update = invoke_node_cb(s, node_idx, event_bytes_ptr);
        }
    } else if is_broadcast_kind {
        // Broadcast path: every node registered for exactly this kind.
        // Pointer kinds never broadcast — a bbox miss stays a miss.
        let mut i = 0usize;
        while i < s.cb_node_kinds.len() {
            if s.cb_node_kinds[i] as u32 == _kind {
                let u = invoke_node_cb(s, i as u32, event_bytes_ptr);
                if u > update {
                    update = u;
                }
            }
            i += 1;
        }
    } else {
        // True pointer miss — nothing to invoke, no patches.
        if _kind == event_kind::MOUSEUP {
            s.pointer_samples.clear();
        }
        core::ptr::write_unaligned(out_len_ptr as usize as *mut u32, 0);
        return 0;
    }

    // 9d-i-a: the pass is over - drain the raw motion the way the desktop
    // pass calls `take_raw_motion` after dispatch, so a delta never re-fires.
    let _ = s.raw_motion.take();
    // 10e-i: iOS clears both sample lists when the touch ends.
    if _kind == event_kind::MOUSEUP {
        s.pointer_samples.clear();
    }

    // M9-5/M9-6: on RefreshDom, encode a SetText TLV patch for the
    // counter display node and return its buffer. The cb has
    // already mutated the model in-place via the refany deref chain,
    // so we just read the updated u32 from `state.model_ptr`,
    // format to decimal, encode the TLV. JS reads the returned
    // `(patch_ptr, patch_len)` and applies via the existing
    // azApplyPatches decoder.
    if update >= UPDATE_REFRESH_DOM && s.model_ptr != 0 && s.display_text_node_idx != u32::MAX {
        // Lazy-allocate the patch buffer (32 bytes covers any
        // SetText: 9 header + ≤10 ASCII digits + slack).
        if s.patch_buf_ptr == 0 {
            s.patch_buf_ptr = AzStartup_alloc(32);
        }
        if s.patch_buf_ptr != 0 {
            let counter = core::ptr::read_unaligned(s.model_ptr as usize as *const u32);
            let used =
                AzStartup_buildCounterPatch(s.patch_buf_ptr, 32, s.display_text_node_idx, counter);
            core::ptr::write_unaligned(out_len_ptr as usize as *mut u32, used);
            return s.patch_buf_ptr;
        }
    }

    // No patches — surface `update` so JS can log it.
    core::ptr::write_unaligned(out_len_ptr as usize as *mut u32, update);
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The loader writes `Math.round(v * 256)`; the wasm side divides back.
    /// A slow mouse reports fractions of a pixel per sample, which integer
    /// pixels would round to nothing at all.
    #[test]
    fn fixed_point_keeps_sub_pixel_motion() {
        assert_eq!(fixed_point::from_f32(0.25), 64);
        assert_eq!(fixed_point::to_f32(64), 0.25);
        assert_eq!(fixed_point::from_f32(-1.5), -384);
        assert_eq!(fixed_point::to_f32(-384), -1.5);
        assert_eq!(fixed_point::to_f32(fixed_point::from_f32(0.0)), 0.0);
    }

    /// The sample tail round-trips through the exact byte layout the JS
    /// encoder writes (`azEncodePointerSamples`), oldest first on both lists.
    #[test]
    fn pointer_samples_round_trip() {
        let coalesced = [[1.0, 2.0], [1.5, 2.5], [2.0, 3.0]];
        let predicted = [[2.5, 3.5], [3.0, 4.0]];
        let tail = pointer_samples::encode(&coalesced, &predicted);
        assert_eq!(
            tail.len(),
            pointer_samples::HEADER_BYTES as usize + pointer_samples::SAMPLE_BYTES as usize * 5
        );

        let mut buf = pointer_samples::PointerSampleBuffer::EMPTY;
        pointer_samples::decode(&tail, &mut buf);
        assert_eq!(buf.coalesced(), &coalesced);
        assert_eq!(buf.predicted(), &predicted);
    }

    /// An empty tail is the mouse-event fallback and "nothing moved between
    /// frames" alike; both decode to no samples, and a previously filled
    /// buffer is cleared rather than left carrying stale points.
    #[test]
    fn an_empty_tail_clears_the_buffer() {
        let mut buf = pointer_samples::PointerSampleBuffer::EMPTY;
        pointer_samples::decode(&pointer_samples::encode(&[[9.0, 9.0]], &[]), &mut buf);
        assert_eq!(buf.coalesced().len(), 1);
        pointer_samples::decode(&[], &mut buf);
        assert!(buf.coalesced().is_empty());
        assert!(buf.predicted().is_empty());
    }

    /// A tail whose counts claim more bytes than were sent, or more samples
    /// than the buffer can hold, decodes to NOTHING - never to the prefix
    /// that happened to fit. A half stroke is a lie about where the pointer
    /// was, and an over-claimed count is exactly the kind of value a stale
    /// bump-allocated buffer would present.
    #[test]
    fn a_truncated_or_overclaimed_tail_decodes_to_nothing() {
        let mut buf = pointer_samples::PointerSampleBuffer::EMPTY;

        let full = pointer_samples::encode(&[[1.0, 1.0], [2.0, 2.0]], &[]);
        pointer_samples::decode(&full[..full.len() - 3], &mut buf);
        assert!(buf.coalesced().is_empty(), "truncated tail must not half-fill");

        let mut overclaimed = pointer_samples::encode(&[[1.0, 1.0]], &[]);
        overclaimed[..4].copy_from_slice(&(pointer_samples::MAX_COALESCED as u32 + 1).to_le_bytes());
        pointer_samples::decode(&overclaimed, &mut buf);
        assert!(buf.coalesced().is_empty(), "count above capacity must be rejected");

        pointer_samples::decode(&[1, 2, 3], &mut buf);
        assert!(buf.coalesced().is_empty(), "a tail shorter than its header is rejected");
    }

    /// The capacities are set by the 256-byte event buffer, and the encoder
    /// truncates the way the loader does: the NEWEST coalesced samples
    /// survive (the segment still being drawn) and the FIRST predicted ones
    /// (a prediction is less trustworthy the further out it reaches).
    #[test]
    fn overflow_keeps_the_newest_coalesced_and_the_first_predicted() {
        let coalesced: Vec<[f32; 2]> = (0..40).map(|i| [i as f32, 0.0]).collect();
        let predicted: Vec<[f32; 2]> = (0..10).map(|i| [100.0 + i as f32, 0.0]).collect();
        let tail = pointer_samples::encode(&coalesced, &predicted);
        assert!(
            event_offset::PAYLOAD as usize + tail.len() <= EVENT_BYTES_LEN as usize,
            "a full tail must still fit the event buffer"
        );

        let mut buf = pointer_samples::PointerSampleBuffer::EMPTY;
        pointer_samples::decode(&tail, &mut buf);
        assert_eq!(buf.coalesced().len(), pointer_samples::MAX_COALESCED);
        assert_eq!(buf.coalesced()[0], [16.0, 0.0], "oldest kept = 40 - 24");
        assert_eq!(buf.coalesced()[23], [39.0, 0.0], "newest kept");
        assert_eq!(buf.predicted().len(), pointer_samples::MAX_PREDICTED);
        assert_eq!(buf.predicted()[0], [100.0, 0.0], "first prediction kept");
    }

    /// Several deltas between passes add up, like `note_raw_motion`, and the
    /// drain empties the accumulator, like `take_raw_motion` - so one delta
    /// can never be delivered twice.
    #[test]
    fn raw_motion_accumulates_then_drains_once() {
        let mut acc = RawMotionAccumulator::NONE;
        assert_eq!(acc.take(), None);
        acc.note(0.5, -0.25, 7);
        acc.note(1.0, 1.0, 7);
        assert_eq!(acc.take(), Some((1.5, 0.75, 7)));
        assert_eq!(acc.take(), None, "drained");
        assert_eq!(acc, RawMotionAccumulator::NONE);
    }

    /// The two new wire kinds are APPENDED after MOUSEOVER: renumbering an
    /// existing kind would silently re-route every event of that kind.
    #[test]
    fn the_pointer_lock_kinds_are_appended() {
        assert_eq!(event_kind::RAWMOUSEMOTION, event_kind::MOUSEOVER + 1);
        assert_eq!(event_kind::POINTERLOCKCHANGE, event_kind::MOUSEOVER + 2);
        assert_eq!(PATCH_KIND_POINTER_LOCK, PATCH_KIND_REMOVE_CLASS + 1);
    }
}
