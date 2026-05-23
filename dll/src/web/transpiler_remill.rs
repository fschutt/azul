//! `RemillTranspiler` — remill-backed implementation of `Transpiler`.
//!
//! Only compiled when the `web-transpiler` Cargo feature is enabled. The
//! feature unlocks the three-stage pipeline:
//!
//! ```text
//!   raw .text bytes
//!     ── remill-lift-17 ──►  LLVM IR (semantics-driven, `%struct.State` form)
//!     ── llc -mtriple=wasm32 -filetype=obj ──►  WASM object
//!     ── wasm-ld --no-entry --export=<sym> ──►  final WASM module
//! ```
//!
//! Isolation requirement: this module sees only `(fn_name, fn_addr, fn_size)`
//! and returns `WasmModule` bytes. It must not depend on any GUI,
//! event-loop, or window types — the caller decides when in the web.md
//! flow lifting happens.
//!
//! Toolchain discovery (in order):
//!   - `$REMILL_LIFT_BIN`, `$LLC`, `$WASM_LD` env vars, then
//!   - `third_party/remill-install/build/remill/bin/lift/remill-lift-17`
//!     and homebrew defaults `/opt/homebrew/opt/{llvm@21,lld@21}/bin/...`
//!     (matches `experiments/transpile-blueprint`'s wiring so artifacts
//!     remain reproducible).
//!
//! Build the remill binary by running `bash scripts/build_remill.sh`
//! (one-time, ~30 min via the bundled cxx-common toolchain). When the
//! binaries are missing, `is_available()` returns `false` and lift
//! methods short-circuit to a structured error so callers fall back to
//! server-side dispatch.

use super::transpiler::{TranspileError, Transpiler, WasmModule};
use super::symbol_table::{self, FnClass as SymFnClass};
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Command;

// ============================================================================
// Callback signature architecture (per user direction 2026-05-18)
// ============================================================================
//
// Goal: per-typedef wrapper synthesis. Every callback type defined in
// api.json (Callback, LayoutCallback, ButtonOnClickCallback,
// CheckBoxOnToggleCallback, NumberInputOnValueChangeCallback,
// ThreadCallback, ...) has a known source-level signature. The M6
// wrapper needs to map that signature to:
//
//   1. A wasm-friendly *exposed* arg list (the JS-side signature).
//   2. The aarch64 PCS placement of each arg in the State struct
//      (which register slots to seed before the lifted body runs).
//   3. The aarch64 PCS placement of the return value (which register
//      to read back, or a hidden-ptr return for large structs).
//
// This file describes the data type + a per-kind lookup. Today only
// `Callback` is fully wired (covers all widget OnClick/Hover/etc.
// callbacks since they share `fn(AzRefAny, AzCallbackInfo) -> AzUpdate`).
// Other kinds default to the Callback shape with a fall-through
// warning — those callbacks will technically dispatch but only the
// first two args (the RefAny halves) will land in the right registers.
// Extending the table per kind is mechanical follow-up work.
//
// The "which kind is this discovered callback?" half lives on the
// discovery side and is currently hardcoded to `Callback`. M7
// extends `DiscoveredCallback` with a typedef tag so per-attachment
// sites (set_on_toggle, layout_callback, etc.) carry their own
// kind through to the lift.

/// PCS placement of a single wrapper arg or return slot.
#[derive(Debug, Clone)]
pub enum Pcs {
    /// Single 64-bit register slot in the GPR struct.
    /// `state_byte_offset` is the byte offset of the X<n> slot
    /// inside `%struct.State` per the lift's GEPs.
    GprI64 { state_byte_offset: u64 },
    /// Two consecutive 64-bit register slots — used for aggregates
    /// >8B and ≤16B (e.g. AzRefAny). The wrapper takes two `i64`
    /// args and stores them into two adjacent X<n> slots.
    GprI64Pair { lo_offset: u64, hi_offset: u64 },
    /// 32-bit wasm pointer, zero-extended into a single X<n> slot.
    /// Used for `*const T` args where T is a struct passed by ptr
    /// (e.g. `*const AzCallbackInfo`).
    GprPtr32 { state_byte_offset: u64 },
    /// 32-bit primitive (e.g. bool, u32, i32) stored in the low
    /// 32 bits of a register slot (W<n>).
    Wreg { state_byte_offset: u64 },
    /// Caller-allocated destination buffer for large struct returns
    /// (>16 bytes via AAPCS64 hidden X8). Only valid as the `ret`
    /// slot of a [`CallbackSignature`] — the wrapper appends one
    /// extra `i32` arg (the wasm-side pointer of the destination
    /// slot), zero-extends it to i64, stores into State.X<8>, and
    /// then returns `i32 0` after the lifted body completes. The
    /// body's `str xN, [x8, #M]` instructions write directly into
    /// the caller's slot. `x8_offset` is the byte offset of the
    /// X<8> slot inside `%struct.State`.
    HiddenPtrReturn { x8_offset: u64 },
}

/// A callback typedef's full wrapper synthesis info.
#[derive(Debug, Clone)]
pub struct CallbackSignature {
    /// Source-level name (`"Callback"`, `"LayoutCallback"`, ...).
    /// Used in error messages + the helper-IR comment block.
    pub kind: String,
    /// PCS placements for each wrapper arg, in source order. The
    /// wrapper's wasm-side parameter list is derived from these.
    pub args: Vec<Pcs>,
    /// PCS placement of the return value. `None` for void returns
    /// (e.g. `ThreadCallback` which returns `()`). For aggregates
    /// >16B this would be `Some(Pcs::HiddenPtr { ... })` once we
    /// add that variant; today only word-sized returns are handled.
    pub ret: Option<Pcs>,
}

/// Look up the wrapper signature for an eventloop function
/// (`AzStartup_<name>`) by its full symbol name. Returns `None` for
/// any non-eventloop symbol so the caller can fall back to the
/// callback-kind table.
///
/// Eventloop signatures follow the C-ABI declared in
/// `dll/src/web/eventloop.rs`:
///
///   AzStartup_alloc(u32 size) -> u32
///   AzStartup_free(u32 ptr, u32 size) -> ()
///   AzStartup_init(u32 json_ptr, u32 json_len) -> u32
///   AzStartup_dispatchEvent(u32 kind, u32 ptr, u32 len) -> u32
///   AzStartup_getPatches(u32 out_ptr, u32 out_cap) -> u32
///   AzStartup_registerStateDeserializer(usize fn_ptr) -> ()
///
/// All u32 args land in W<n> (low 32 bits of X<n>) per AArch64 PCS.
/// `usize` on AArch64 is 64-bit; we use `Pcs::GprI64` for the
/// `registerStateDeserializer` arg so the full address survives —
/// the lifted body stores it via i64 ops, and the wrapper exposes a
/// 64-bit JS-side parameter.
pub fn signature_for_eventloop_fn(name: &str) -> Option<CallbackSignature> {
    // X<n> slot byte offsets inside %struct.State per the lift's GEPs.
    const X0: u64 = 544;
    const X1: u64 = 560;
    const X2: u64 = 576;
    const X3: u64 = 592;
    const X4: u64 = 608;
    match name {
        "AzStartup_alloc" => Some(CallbackSignature {
            kind: "AzStartup_alloc".to_string(),
            args: vec![Pcs::Wreg { state_byte_offset: X0 }],
            ret: Some(Pcs::Wreg { state_byte_offset: X0 }),
        }),
        "AzStartup_free" => Some(CallbackSignature {
            kind: "AzStartup_free".to_string(),
            args: vec![
                Pcs::Wreg { state_byte_offset: X0 },
                Pcs::Wreg { state_byte_offset: X1 },
            ],
            ret: None,
        }),
        "AzStartup_init" => Some(CallbackSignature {
            kind: "AzStartup_init".to_string(),
            // (json_ptr: u32, json_len: u32) -> state_ptr: u32
            args: vec![
                Pcs::Wreg { state_byte_offset: X0 },
                Pcs::Wreg { state_byte_offset: X1 },
            ],
            ret: Some(Pcs::Wreg { state_byte_offset: X0 }),
        }),
        "AzStartup_hydrate" => Some(CallbackSignature {
            kind: "AzStartup_hydrate".to_string(),
            // (type_id_lo: u32, type_id_hi: u32, data_ptr: u32,
            //  data_size: u32) -> refany_ptr: u32
            args: vec![
                Pcs::Wreg { state_byte_offset: X0 },
                Pcs::Wreg { state_byte_offset: X1 },
                Pcs::Wreg { state_byte_offset: X2 },
                Pcs::Wreg { state_byte_offset: X3 },
            ],
            ret: Some(Pcs::Wreg { state_byte_offset: X0 }),
        }),
        "AzStartup_dispatchEvent" => Some(CallbackSignature {
            kind: "AzStartup_dispatchEvent".to_string(),
            // (state, kind, evt_ptr, evt_len, out_len_ptr) -> patches_ptr
            args: vec![
                Pcs::Wreg { state_byte_offset: X0 },
                Pcs::Wreg { state_byte_offset: X1 },
                Pcs::Wreg { state_byte_offset: X2 },
                Pcs::Wreg { state_byte_offset: X3 },
                Pcs::Wreg { state_byte_offset: X4 },
            ],
            ret: Some(Pcs::Wreg { state_byte_offset: X0 }),
        }),
        "AzStartup_registerStateDeserializer" => Some(CallbackSignature {
            kind: "AzStartup_registerStateDeserializer".to_string(),
            // (state: u32, fn_addr: u64) -> ()
            args: vec![
                Pcs::Wreg { state_byte_offset: X0 },
                Pcs::GprI64 { state_byte_offset: X1 },
            ],
            ret: None,
        }),
        "AzStartup_buildLayoutInfo" => Some(CallbackSignature {
            kind: "AzStartup_buildLayoutInfo".to_string(),
            // (viewport_w: u32, viewport_h: u32, theme: u32) -> info_ptr: u32
            args: vec![
                Pcs::Wreg { state_byte_offset: X0 },
                Pcs::Wreg { state_byte_offset: X1 },
                Pcs::Wreg { state_byte_offset: X2 },
            ],
            ret: Some(Pcs::Wreg { state_byte_offset: X0 }),
        }),
        "AzStartup_setLayoutCbTableIdx" => Some(CallbackSignature {
            kind: "AzStartup_setLayoutCbTableIdx".to_string(),
            // (state: u32, idx: u32) -> ()
            args: vec![
                Pcs::Wreg { state_byte_offset: X0 },
                Pcs::Wreg { state_byte_offset: X1 },
            ],
            ret: None,
        }),
        "AzStartup_setRefAny" => Some(CallbackSignature {
            kind: "AzStartup_setRefAny".to_string(),
            // (state: u32, refany_ptr: u32) -> ()
            args: vec![
                Pcs::Wreg { state_byte_offset: X0 },
                Pcs::Wreg { state_byte_offset: X1 },
            ],
            ret: None,
        }),
        "AzStartup_initLayoutCache" => Some(CallbackSignature {
            kind: "AzStartup_initLayoutCache".to_string(),
            // (state: u32, viewport_w: u32, viewport_h: u32, theme: u32)
            // -> status: u32
            args: vec![
                Pcs::Wreg { state_byte_offset: X0 },
                Pcs::Wreg { state_byte_offset: X1 },
                Pcs::Wreg { state_byte_offset: X2 },
                Pcs::Wreg { state_byte_offset: X3 },
            ],
            ret: Some(Pcs::Wreg { state_byte_offset: X0 }),
        }),
        "AzStartup_getCurrentDomPtr" | "AzStartup_getLastLayoutStatus" => Some(CallbackSignature {
            kind: name.to_string(),
            // (state: u32) -> u32
            args: vec![Pcs::Wreg { state_byte_offset: X0 }],
            ret: Some(Pcs::Wreg { state_byte_offset: X0 }),
        }),
        "AzStartup_registerCbNode" => Some(CallbackSignature {
            kind: "AzStartup_registerCbNode".to_string(),
            // (state: u32, node_idx: u32) -> ()
            args: vec![
                Pcs::Wreg { state_byte_offset: X0 },
                Pcs::Wreg { state_byte_offset: X1 },
            ],
            ret: None,
        }),
        "AzStartup_hitTest" => Some(CallbackSignature {
            kind: "AzStartup_hitTest".to_string(),
            // (state: u32, x_bits: u32, y_bits: u32) -> node_idx: u32
            args: vec![
                Pcs::Wreg { state_byte_offset: X0 },
                Pcs::Wreg { state_byte_offset: X1 },
                Pcs::Wreg { state_byte_offset: X2 },
            ],
            ret: Some(Pcs::Wreg { state_byte_offset: X0 }),
        }),
        "AzStartup_buildCounterPatch" => Some(CallbackSignature {
            kind: "AzStartup_buildCounterPatch".to_string(),
            // (out_buf: u32, out_buf_cap: u32, node_idx: u32,
            //  counter_value: u32) -> used_bytes: u32
            args: vec![
                Pcs::Wreg { state_byte_offset: X0 },
                Pcs::Wreg { state_byte_offset: X1 },
                Pcs::Wreg { state_byte_offset: X2 },
                Pcs::Wreg { state_byte_offset: X3 },
            ],
            ret: Some(Pcs::Wreg { state_byte_offset: X0 }),
        }),
        "AzStartup_setModelPtr" | "AzStartup_setDisplayNode" => Some(CallbackSignature {
            kind: name.to_string(),
            // (state: u32, value: u32) -> ()
            args: vec![
                Pcs::Wreg { state_byte_offset: X0 },
                Pcs::Wreg { state_byte_offset: X1 },
            ],
            ret: None,
        }),
        // M11 Sprint 1 — hydrate + getters (cascade + diff cross-check).
        "AzStartup_hydrateStyledDom"
        | "AzStartup_isStyledDomHydrated"
        | "AzStartup_getDomNodeCount"
        | "AzStartup_getStyledDomNodeCount"
        | "AzStartup_getStyledDomPtr"
        | "AzStartup_isLayoutSolved"
        | "AzStartup_getPositionedRectsLen"
        | "AzStartup_getPositionedRectsPtr" => Some(CallbackSignature {
            kind: name.to_string(),
            // (state: u32) -> u32
            args: vec![Pcs::Wreg { state_byte_offset: X0 }],
            ret: Some(Pcs::Wreg { state_byte_offset: X0 }),
        }),
        "AzStartup_solveLayout" | "AzStartup_solveLayoutReal" => Some(CallbackSignature {
            kind: name.to_string(),
            // (state: u32, viewport_w: u32, viewport_h: u32) -> u32
            args: vec![
                Pcs::Wreg { state_byte_offset: X0 },
                Pcs::Wreg { state_byte_offset: X1 },
                Pcs::Wreg { state_byte_offset: X2 },
            ],
            ret: Some(Pcs::Wreg { state_byte_offset: X0 }),
        }),
        // M11 Sprint 3 — relayout (state-only) + buildPatch (6 args).
        "AzStartup_relayout" | "AzStartup_getAutoVirtualizeThreshold"
        | "AzStartup_getCascadeProbe" => Some(CallbackSignature {
            kind: name.to_string(),
            args: vec![Pcs::Wreg { state_byte_offset: X0 }],
            ret: Some(Pcs::Wreg { state_byte_offset: X0 }),
        }),
        // M11 Sprint 5 — VirtualView setters (state, u32) -> ().
        "AzStartup_setAutoVirtualizeThreshold"
        | "AzStartup_setVirtualViewProvider"
        | "AzStartup_pokeLastLayout" => Some(CallbackSignature {
            kind: name.to_string(),
            args: vec![
                Pcs::Wreg { state_byte_offset: X0 },
                Pcs::Wreg { state_byte_offset: X1 },
            ],
            ret: None,
        }),
        "AzStartup_buildPatch" => Some(CallbackSignature {
            kind: name.to_string(),
            // (out_buf, out_buf_cap, kind, node_idx, payload_ptr,
            //  payload_len) -> total_bytes
            args: vec![
                Pcs::Wreg { state_byte_offset: X0 },
                Pcs::Wreg { state_byte_offset: X1 },
                Pcs::Wreg { state_byte_offset: X2 },
                Pcs::Wreg { state_byte_offset: X3 },
                Pcs::Wreg { state_byte_offset: X4 },
                Pcs::Wreg { state_byte_offset: 624 }, // X5
            ],
            ret: Some(Pcs::Wreg { state_byte_offset: X0 }),
        }),
        _ => None,
    }
}

/// Look up the wrapper signature for a callback typedef by its
/// short name (without the trailing `Type` — i.e. `Callback`, not
/// `CallbackType`). Returns the canonical `Callback` shape for
/// any unrecognized name, so the lift pipeline keeps working when
/// new typedefs are added to api.json before this table catches
/// up — at the cost of mis-placed args for kinds with extra
/// params (CheckBoxOnToggle's bool, NumberInput's i32, etc.).
pub fn signature_for_callback_kind(kind: &str) -> CallbackSignature {
    // aarch64 State layout (per the GEPs the lift emits for
    // `%struct.State`'s GPR substruct). The struct alternates
    // i64 padding + Reg unions; X<n> ends up at offset
    // `544 + n * 16` for the registers we care about. SP is at
    // +1040, X29/X30 at +1008/+1024.
    const X0: u64 = 544;
    const X1: u64 = 560;
    const X2: u64 = 576;
    const X3: u64 = 592;
    // X8 is the Indirect Result Location Register per AAPCS64 —
    // the caller writes the destination-buffer pointer into X8
    // before the call, and the callee's `str xN, [x8, #M]`
    // instructions write the (large) struct return through it.
    // Used by [`HiddenPtrReturn`] for LayoutCallback (which
    // returns `AzDom`, > 16 bytes).
    const X8: u64 = 672;
    let canonical_callback = || CallbackSignature {
        kind: "Callback".to_string(),
        // `extern "C" fn(AzRefAny, AzCallbackInfo) -> AzUpdate`
        // AzRefAny: 16B aggregate → X0+X1 pair.
        // AzCallbackInfo: >16B → *const passed in X2.
        // AzUpdate: 4B enum → W0 (low 32 bits of X0).
        args: vec![
            Pcs::GprI64Pair { lo_offset: X0, hi_offset: X1 },
            Pcs::GprPtr32 { state_byte_offset: X2 },
        ],
        ret: Some(Pcs::Wreg { state_byte_offset: X0 }),
    };
    match kind {
        "Callback"
        | "ButtonOnClickCallback"
        | "TabOnClickCallback"
        | "TreeViewOnNodeClickCallback"
        | "DropDownOnChoiceChangeCallback"
        | "RibbonOnTabClickCallback" => canonical_callback(),
        "CheckBoxOnToggleCallback" => CallbackSignature {
            kind: "CheckBoxOnToggleCallback".to_string(),
            // ...same as Callback, plus a trailing `bool` arg in X3.
            args: vec![
                Pcs::GprI64Pair { lo_offset: X0, hi_offset: X1 },
                Pcs::GprPtr32 { state_byte_offset: X2 },
                Pcs::Wreg { state_byte_offset: X3 },
            ],
            ret: Some(Pcs::Wreg { state_byte_offset: X0 }),
        },
        "LayoutCallback" => CallbackSignature {
            kind: "LayoutCallback".to_string(),
            // `extern "C" fn(AzRefAny, AzLayoutCallbackInfo) -> AzDom`
            // AzRefAny: 16B aggregate → X0+X1 pair.
            // AzLayoutCallbackInfo: >16B → *const passed in X2.
            // AzDom: large aggregate return → hidden X8 destination
            // pointer (caller-allocated).
            //
            // The wrapper appends a 4th `i32 out_ptr` arg, writes
            // State.X8 from it, and returns `i32 0` after the body
            // populates the caller's buffer. JS-side signature is
            // `(refany_lo: i64, refany_hi: i64, info_ptr: i32,
            //   out_ptr: i32) -> i32`.
            args: vec![
                Pcs::GprI64Pair { lo_offset: X0, hi_offset: X1 },
                Pcs::GprPtr32 { state_byte_offset: X2 },
            ],
            ret: Some(Pcs::HiddenPtrReturn { x8_offset: X8 }),
        },
        _ => {
            eprintln!(
                "[azul-web] callback kind {:?} not in signature_for_callback_kind() — \
                 falling back to canonical Callback shape; first two args + return will \
                 dispatch correctly but extra args + struct returns will be wrong",
                kind
            );
            canonical_callback()
        }
    }
}

/// Build the wrapper-arg list (wasm-side parameter declarations,
/// LLVM IR syntax) and the prologue (stores into the State alloca)
/// from a CallbackSignature.
///
/// When `sig.ret` is [`Pcs::HiddenPtrReturn`], one extra trailing
/// `i32 %out_ptr` parameter is appended and the prologue zero-extends
/// it into the X8 (Indirect Result Location) slot of the State
/// struct. The lifted body's `str xN, [x8, #M]` instructions then
/// write through that pointer into the caller's destination buffer.
fn emit_wrapper_args_and_prologue(sig: &CallbackSignature) -> (String, String) {
    let mut params: Vec<String> = Vec::new();
    let mut prologue = String::new();
    for (i, pcs) in sig.args.iter().enumerate() {
        match pcs {
            Pcs::GprI64 { state_byte_offset } => {
                params.push(format!("i64 %arg{}", i));
                prologue.push_str(&format!(
                    "  %arg{i}_p = getelementptr inbounds i8, ptr %state_buf, i64 {off}\n",
                    i = i,
                    off = state_byte_offset
                ));
                prologue.push_str(&format!(
                    "  store i64 %arg{i}, ptr %arg{i}_p, align 8\n",
                    i = i
                ));
            }
            Pcs::GprI64Pair { lo_offset, hi_offset } => {
                params.push(format!("i64 %arg{}_lo", i));
                params.push(format!("i64 %arg{}_hi", i));
                prologue.push_str(&format!(
                    "  %arg{i}_lo_p = getelementptr inbounds i8, ptr %state_buf, i64 {lo}\n  \
                       %arg{i}_hi_p = getelementptr inbounds i8, ptr %state_buf, i64 {hi}\n  \
                       store i64 %arg{i}_lo, ptr %arg{i}_lo_p, align 8\n  \
                       store i64 %arg{i}_hi, ptr %arg{i}_hi_p, align 8\n",
                    i = i,
                    lo = lo_offset,
                    hi = hi_offset
                ));
            }
            Pcs::GprPtr32 { state_byte_offset } => {
                params.push(format!("i32 %arg{}", i));
                prologue.push_str(&format!(
                    "  %arg{i}_p = getelementptr inbounds i8, ptr %state_buf, i64 {off}\n  \
                       %arg{i}_i64 = zext i32 %arg{i} to i64\n  \
                       store i64 %arg{i}_i64, ptr %arg{i}_p, align 8\n",
                    i = i,
                    off = state_byte_offset
                ));
            }
            Pcs::Wreg { state_byte_offset } => {
                params.push(format!("i32 %arg{}", i));
                prologue.push_str(&format!(
                    "  %arg{i}_p = getelementptr inbounds i8, ptr %state_buf, i64 {off}\n  \
                       store i32 %arg{i}, ptr %arg{i}_p, align 4\n",
                    i = i,
                    off = state_byte_offset
                ));
            }
            // HiddenPtrReturn is only valid as `sig.ret`, not an arg —
            // ignore here; the trailing-arg emission below handles it.
            Pcs::HiddenPtrReturn { .. } => {
                eprintln!(
                    "[azul-web] BUG: Pcs::HiddenPtrReturn appeared in sig.args (only valid as sig.ret); skipping",
                );
            }
        }
    }
    // If the return uses a hidden destination pointer, append the
    // pointer as an extra `i32` arg AND seed State.X8 with it.
    if let Some(Pcs::HiddenPtrReturn { x8_offset }) = sig.ret.as_ref() {
        params.push("i32 %out_ptr".to_string());
        prologue.push_str(&format!(
            "  ; HiddenPtrReturn: store out_ptr → State.X8 (AAPCS64 IRL)\n  \
               %out_ptr_i64 = zext i32 %out_ptr to i64\n  \
               %x8_p = getelementptr inbounds i8, ptr %state_buf, i64 {off}\n  \
               store i64 %out_ptr_i64, ptr %x8_p, align 8\n",
            off = x8_offset
        ));
    }
    (params.join(", "), prologue)
}

/// Build the return-type fragment and post-call return-read code from
/// the signature's return PCS.
///
/// [`Pcs::HiddenPtrReturn`] is wired here as `i32` returning a status
/// code (`0 = ok`). The actual struct return was written by the
/// lifted body through the X8-seeded destination pointer (see the
/// `out_ptr` arg appended in [`emit_wrapper_args_and_prologue`]),
/// so there's nothing to load back from State here.
fn emit_wrapper_return(sig: &CallbackSignature) -> (String, String) {
    match sig.ret.as_ref() {
        None => ("void".to_string(), String::from("  ret void\n")),
        Some(Pcs::Wreg { state_byte_offset }) => (
            "i32".to_string(),
            format!(
                "  %ret_p = getelementptr inbounds i8, ptr %state_buf, i64 {off}\n  \
                   %ret_w = load i32, ptr %ret_p, align 4\n  \
                   ret i32 %ret_w\n",
                off = state_byte_offset
            ),
        ),
        Some(Pcs::GprI64 { state_byte_offset }) => (
            "i64".to_string(),
            format!(
                "  %ret_p = getelementptr inbounds i8, ptr %state_buf, i64 {off}\n  \
                   %ret_x = load i64, ptr %ret_p, align 8\n  \
                   ret i64 %ret_x\n",
                off = state_byte_offset
            ),
        ),
        Some(Pcs::HiddenPtrReturn { .. }) => (
            "i32".to_string(),
            "  ; HiddenPtrReturn: body wrote the struct through X8 →\n  \
               ; caller's destination buffer. Return status=0 (ok).\n  \
               ret i32 0\n"
                .to_string(),
        ),
        // Pair returns left for the M7 generalization pass; canonical
        // Callback shape never hits this.
        Some(other) => {
            eprintln!(
                "[azul-web] callback return PCS {:?} not yet wired — defaulting to i32 X0",
                other
            );
            (
                "i32".to_string(),
                "  %ret_p = getelementptr inbounds i8, ptr %state_buf, i64 544\n  \
                   %ret_w = load i32, ptr %ret_p, align 4\n  \
                   ret i32 %ret_w\n"
                    .to_string(),
            )
        }
    }
}

/// remill-backed transpiler. Holds the resolved tool paths so each
/// lift call doesn't redo discovery.
pub struct RemillTranspiler {
    /// Resolved path to `remill-lift-17`. `None` when the binary
    /// isn't discoverable; lift methods return an error in that case.
    remill_lift: Option<PathBuf>,
    /// Resolved path to LLVM `llc` (must understand `wasm32` target).
    llc: Option<PathBuf>,
    /// Resolved path to LLVM `opt` (M6 IR cleanup pass).
    opt: Option<PathBuf>,
    /// Resolved path to LLVM `llvm-link` (M6 helper merge).
    llvm_link: Option<PathBuf>,
    /// Resolved path to `wasm-ld` (ships with `lld`, not `llvm`).
    wasm_ld: Option<PathBuf>,
    /// Output scratch directory; defaults to
    /// `$TMPDIR/azul-web-transpiler-<pid>`. Created on first lift call.
    scratch_dir: PathBuf,
    /// Per-`(canonical_addr, export_as)` cache of produced .o files.
    /// Without this, the recursive transitive lift redoes work for
    /// every callback that shares deps — layout cb and on_click both
    /// drag in AzRefCount_clone, AzRefAny_isType, …; each per-cb
    /// transitive walk re-spawned remill+opt+llc+llvm-link for the
    /// same fn. The cache makes the layout-cb lift's 40+ deps lift
    /// once globally instead of once per callback that needs them.
    /// Keyed by (canonical_addr, export_as) because the same fn can
    /// be exported under different names (root → `callback`, dep →
    /// `__az_dep_<addr>`) and the produced .o's export differs.
    object_cache: std::sync::Mutex<
        std::collections::HashMap<(usize, String), PathBuf>,
    >,
}

impl RemillTranspiler {
    pub fn new() -> Self {
        let scratch_dir = std::env::temp_dir().join(format!(
            "azul-web-transpiler-{}",
            std::process::id()
        ));
        Self {
            remill_lift: discover_remill_lift(),
            llc: discover_llc(),
            opt: discover_opt(),
            llvm_link: discover_llvm_link(),
            wasm_ld: discover_wasm_ld(),
            scratch_dir,
            object_cache: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Whether to keep scratch artifacts after the transpiler drops.
    /// `AZ_REMILL_KEEP_SCRATCH=1` retains the per-fn .lifted.ll /
    /// .patched.ll / .helper.ll / .o / .wasm files for post-mortem
    /// debugging. Default behavior wipes them — the build cycle of
    /// the M8.9 audit left ~95 MB of stale scratch dirs around.
    fn should_keep_scratch() -> bool {
        std::env::var_os("AZ_REMILL_KEEP_SCRATCH").is_some()
    }

    /// Whether the in-process remill+LLVM+LLD pipeline should be used
    /// in place of the subprocess `remill-lift-17` + `opt` + `llc` +
    /// `wasm-ld` chain.
    ///
    /// Opt-in via `AZ_NATIVE_REMILL=1` (requires the build-time
    /// `web-transpiler-static` feature, which statically links
    /// remill+LLVM+LLD into libazul.dylib).
    ///
    /// **NOT default**, despite sharing one `LoadArchSemantics`: the
    /// in-process `compile_to_wasm32_obj` merges *every* lifted fn into
    /// ONE module and runs `opt -O2` on it. For the ~1607-fn layout
    /// graph that single giant-module optimization is both slow
    /// (≈7 min, 1.35 GB peak) AND miscompiles — verified on the layout
    /// lift it regresses the cascade to `getStyledDomNodeCount == 0`
    /// (recvec/u128 scrambled, plus LLVM "Linking two modules of
    /// different target triples/datalayouts" warnings: one input keeps
    /// the host aarch64 datalayout instead of wasm32). The subprocess
    /// chain lifts the same graph correctly in ≈2.4 min with bounded
    /// per-fn memory. The right native path is the **sharded** design
    /// (1 fn → 1 wasm with linker imports, opt per small module) — once
    /// that lands + the triple/datalayout normalization is fixed, this
    /// can flip back to default. Until then subprocess is the default.
    fn use_native_remill(&self) -> bool {
        cfg!(feature = "web-transpiler-static")
            && std::env::var_os("AZ_NATIVE_REMILL").is_some()
    }

    /// Return the full toolchain or a structured TranspileError naming
    /// the first missing binary.
    fn tools(&self, fn_name: &str) -> Result<Tools<'_>, TranspileError> {
        let remill_lift = self.remill_lift.as_deref().ok_or_else(|| TranspileError {
            fn_name: fn_name.to_string(),
            reason: "remill-lift-17 not found — set $REMILL_LIFT_BIN or run scripts/build_remill.sh"
                .into(),
        })?;
        let llc = self.llc.as_deref().ok_or_else(|| TranspileError {
            fn_name: fn_name.to_string(),
            reason: "llc not found — set $LLC or install LLVM 21".into(),
        })?;
        let wasm_ld = self.wasm_ld.as_deref().ok_or_else(|| TranspileError {
            fn_name: fn_name.to_string(),
            reason: "wasm-ld not found — set $WASM_LD or install lld 21".into(),
        })?;
        Ok(Tools {
            remill_lift,
            llc,
            wasm_ld,
        })
    }

    /// Run the M5-M7 pipeline through `llc` and return the produced
    /// wasm32 object path. The caller decides whether to wasm-ld it
    /// into a self-contained `.wasm` (per-callback case, via
    /// [`pipeline_single`]) or batch several objects through one
    /// wasm-ld invocation (eventloop case, via
    /// [`lift_and_link_eventloop`]).
    ///
    /// `lift_addr` MUST be unique across calls that will be linked
    /// together — remill names the lifted top-level function
    /// `@sub_<lift_addr_hex>`, and the wrapper IR `@call`s it under
    /// that name. If two .o files share a lift_addr, wasm-ld will
    /// see colliding `sub_<hex>` symbols.
    fn produce_object_for(
        &self,
        fn_name: &str,
        fn_addr: usize,
        fn_size: usize,
        sig: &CallbackSignature,
        export_as: &str,
        lift_addr: u64,
    ) -> Result<PathBuf, TranspileError> {
        let raw_lifted_ir = self.lift_fn(fn_name, fn_addr, fn_size, lift_addr)?;
        self.produce_object_from_lifted_ir(
            fn_name, fn_addr, lift_addr, sig, export_as, &raw_lifted_ir,
        )
    }

    /// Lift a single function to its raw remill IR (one `define ptr
    /// @sub_<lift_addr_hex>(...)` plus extern declarations for bl
    /// targets). Used by `produce_object_for` for the per-fn path;
    /// batched-lift call sites bypass this and use
    /// `native_remill::lift_batch` directly to share LoadArchSemantics.
    fn lift_fn(
        &self,
        fn_name: &str,
        fn_addr: usize,
        fn_size: usize,
        lift_addr: u64,
    ) -> Result<String, TranspileError> {
        let use_native = self.use_native_remill();
        std::fs::create_dir_all(&self.scratch_dir).map_err(|e| TranspileError {
            fn_name: fn_name.to_string(),
            reason: format!("scratch dir: {e}"),
        })?;
        // SAFETY: caller asserts (fn_addr, fn_size) cover live .text
        // bytes (typically derived from the SymbolTable's exact
        // `next_symbol_addr - this_addr` slice).
        let mut bytes: Vec<u8> = unsafe {
            std::slice::from_raw_parts(fn_addr as *const u8, fn_size).to_vec()
        };
        rewrite_ldapr_to_ldar(&mut bytes);
        rewrite_recursive_bl(&mut bytes);
        let arch_tag = host_arch_tag().ok_or_else(|| TranspileError {
            fn_name: fn_name.to_string(),
            reason: "unsupported host architecture for remill (need aarch64 or x86_64)".into(),
        })?;
        let stem = sanitize_filename(fn_name);
        let lifted_ir_path = self.scratch_dir.join(format!("{}.lifted.ll", stem));
        // On-disk lift cache (subprocess path only — the native path is
        // already spawn-free). A hit skips the remill-lift-17 subprocess,
        // the slowest per-fn step; the IR is synth-addressed so it stays
        // valid across restarts + dll relinks that don't touch this fn's
        // machine bytes. `bytes` here is already post-rewrite.
        let cache_path = if !use_native && std::env::var_os("AZ_NO_LIFT_CACHE").is_none() {
            Some(lift_cache_path(&bytes, lift_addr))
        } else {
            None
        };
        if let Some(ref cp) = cache_path {
            if let Ok(ir) = std::fs::read_to_string(cp) {
                // Mirror into scratch so downstream stem-based reads work.
                let _ = std::fs::write(&lifted_ir_path, &ir);
                return Ok(ir);
            }
        }
        if use_native {
            #[cfg(feature = "web-transpiler-static")]
            {
                let ir = super::native_remill::lift(
                    arch_tag, host_os_tag(), lift_addr, &bytes,
                )
                .map_err(|e| TranspileError {
                    fn_name: fn_name.to_string(),
                    reason: format!("native lift: {}", e),
                })?;
                std::fs::write(&lifted_ir_path, &ir).map_err(|e| TranspileError {
                    fn_name: fn_name.to_string(),
                    reason: format!("write lifted IR: {e}"),
                })?;
                return Ok(ir);
            }
            #[cfg(not(feature = "web-transpiler-static"))]
            unreachable!("use_native_remill() returns false without the feature");
        }
        let tools = self.tools(fn_name)?;
        let hex = bytes_to_hex(&bytes);
        run_tool(
            tools.remill_lift,
            &[
                "--arch", arch_tag,
                "--os", host_os_tag(),
                "--address", &format!("0x{:x}", lift_addr),
                "--entry_address", &format!("0x{:x}", lift_addr),
                "--bytes", &hex,
                "--ir_out", lifted_ir_path.to_str().expect("scratch path is utf-8"),
            ],
            fn_name,
        )?;
        let ir = std::fs::read_to_string(&lifted_ir_path).map_err(|e| TranspileError {
            fn_name: fn_name.to_string(),
            reason: format!("read lifted IR: {e}"),
        })?;
        // Store the freshly-lifted IR in the on-disk cache for future runs.
        if let Some(ref cp) = cache_path {
            if let Some(parent) = cp.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(cp, &ir);
        }
        Ok(ir)
    }

    /// Post-lift pipeline: takes a `raw_lifted_ir` (from `lift_fn` for
    /// single lifts or from `native_remill::lift_batch` for batched
    /// lifts), applies SymbolTable name rewriting, emits the per-fn
    /// helper IR with the wrapper + branch-extern bodies, then
    /// compiles to a wasm32 .o object. Returns the path to the .o.
    fn produce_object_from_lifted_ir(
        &self,
        fn_name: &str,
        fn_addr: usize,
        lift_addr: u64,
        sig: &CallbackSignature,
        export_as: &str,
        raw_lifted_ir: &str,
    ) -> Result<PathBuf, TranspileError> {
        let use_native = self.use_native_remill();
        let tools = if use_native {
            None
        } else {
            Some(self.tools(fn_name)?)
        };
        std::fs::create_dir_all(&self.scratch_dir).map_err(|e| TranspileError {
            fn_name: fn_name.to_string(),
            reason: format!("scratch dir: {e}"),
        })?;
        // Stem on `export_as` (not fn_name) because two different
        // canonical-addr targets can resolve to the SAME fn_name in
        // the SymbolTable (e.g. aliased / multiply-monomorphized Rust
        // generics like `CssProperty::clone`). export_as is unique
        // per (canonical_addr, role) — deps use `__az_dep_<addr_hex>`,
        // roots use the caller-chosen name. Without this, both .o
        // files would land at the same path; the second overwrites
        // the first and both paths get pushed into object_paths,
        // producing a wasm-ld "duplicate symbol" error.
        let stem = sanitize_filename(export_as);
        // Stash the raw lifted IR for debugging (key on fn_name + addr
        // so the dump is identifiable in scratch listings).
        let _ = std::fs::write(
            self.scratch_dir.join(format!("{}_{:x}.lifted.ll", sanitize_filename(fn_name), fn_addr)),
            raw_lifted_ir,
        );

        // M6 — IR cleanup phase.
        //
        // 1. Patch the lifted IR to mark `sub_<entry>` as `alwaysinline`.
        //    remill emits it as a top-level export by default; without
        //    `alwaysinline` opt's inliner won't pull it into the wrapper
        //    and SROA can't evaporate the State alloca.
        //
        // 2. Generate a helper module with bodies for `__remill_*`
        //    intrinsics (memory ops → real load/store, control intrinsics →
        //    noop) AND a `callback` wrapper that allocates the State
        //    struct on the stack, seeds the arg registers, calls the
        //    lifted function, reads the return register.
        //
        // 3. `llvm-link` lifted + helper → merged module.
        //
        // 4. `opt -O2` — inlines `sub_<entry>` into `callback`, SROA
        //    splits the State alloca into individual register slots,
        //    mem2reg promotes to SSA, dead-code elimination drops the
        //    PC bookkeeping and `__remill_*` noop calls. Typical wasm
        //    size reduction: 50-80%.
        //
        // 5. `llc -mtriple=wasm32` on the cleaned IR.

        // M8.8 Stage 1: rewrite every `@sub_<hex>[.N]` token to use
        // the symbol's canonical post-PLT-chase address as its hex.
        // Eliminates the `.N` suffix (every call site to the same
        // callee dedupes to one canonical name), the PLT-stub thunk
        // mismatch (caller sees the same name the dep lift defined
        // under), and bare-`b` shim mismatches (table chain
        // pre-redirects the shim → target). See module docs.
        //
        // Follow-up dedup pass: when the rewriter collapses two
        // originally-distinct externs (`sub_<addr>` and
        // `sub_<addr>.2`) to the same canonical name, the IR ends up
        // with two identical `declare ptr @sub_<canonical>(...)`
        // lines. llvm-link rejects that as a redefinition. The dedup
        // pass collapses repeated declares to one.
        let lifted_ir = match symbol_table::get() {
            Some(table) => {
                let rewritten =
                    rewrite_sub_names_to_canonical(raw_lifted_ir, table, fn_addr, lift_addr);
                dedup_sub_declares(&rewritten)
            }
            None => raw_lifted_ir.to_string(),
        };
        // M10-B1.a: tag every load/store in the lifted IR with the
        // host alias-scope so LLVM's ScopedAA can prove the lifted
        // body's State / local-alloca accesses don't alias the
        // helper IR's guest inttoptr accesses. SROA on the State
        // alloca becomes viable post-link → layout cb size drops.
        let lifted_ir = tag_state_accesses(&lifted_ir);

        // M12.5d: retarget the AArch64 IR header to wasm32 BEFORE
        // opt/llc. Without this, LLVM uses AArch64 datalayout (i64
        // pointers) for SROA/InstCombine, then llc retargets at
        // emission. The mismatch causes SROA to split the State
        // struct using i64-pointer arithmetic, and the resulting
        // wasm function signatures have i64 state args (e.g.
        // create_from_compact_dom shows `(i64, i64, i32) -> i32`
        // when it should be `(i32, i64, i32) -> i32`). Pointer math
        // happens in i64 with i32.wrap_i64 at memory ops — this
        // breaks cross-function state-ptr propagation and is the
        // root cause of cascade-output sret writes going to wrong
        // wasm addresses.
        let lifted_ir = retarget_to_wasm32(&lifted_ir);
        // M12.5d-fix: strip `noalias` from sub_* fn args (see fn
        // docs above). Without this, LLVM's cross-fn AA corrupts
        // const-pool reads in unrelated lifted bodies whenever
        // ANY lifted fn has a `(&mut T) -> _` signature.
        let lifted_ir = strip_noalias_from_sub_args(&lifted_ir);

        // M9-review: after `rewrite_sub_names_to_canonical`, every
        // `sub_<hex>` reference in the IR uses the CANONICAL SYNTH
        // address as its hex. To find the entry's `define` line for
        // `inject_alwaysinline`, look up fn_addr's synthetic_addr +
        // chase the synth chain to its canonical synth target.
        let canonical_entry_addr = symbol_table::get()
            .and_then(|t| {
                t.lookup(fn_addr)
                    .map(|e| t.resolve_synth(e.synthetic_addr).unwrap_or(e.synthetic_addr))
            })
            .unwrap_or(fn_addr) as u64;

        // For AzStartup_* eventloop functions we deliberately do NOT
        // inject alwaysinline on the lifted body. Reason: with
        // alwaysinline, opt -O2 inlines the lifted body into the
        // wrapper and then aggressively simplifies away the
        // resolve+call chain (the wrapper's State alloca doesn't
        // escape so opt treats every State-flowing operation as
        // potentially dead — even volatile stores to global anti-DCE
        // sinks get eliminated through some inliner-internal
        // simplification pass). Without alwaysinline the wrapper
        // ends up calling sub_<lift_addr> via a normal call →
        // call chain stays observable.
        //
        // The downside is size: the per-fn wrapper's State alloca
        // stays as a real 1088-byte stack buffer instead of being
        // SROA'd into individual register slots. Acceptable for the
        // ~7 eventloop functions; the M5-M7 per-callback path keeps
        // alwaysinline since hello-world's on_click has no
        // observable side effects past its return.
        //
        // M10-D: boundary-lift roots use the same `AzStartup_` /
        // `AzBoundary_` skip-alwaysinline path. Boundary shards
        // EXPORT the raw `sub_<canonical_hex>` body so other wasms
        // can import it. With alwaysinline on the entry, opt -O2
        // inlines the body into the (unused) wrapper, then
        // --gc-sections strips the wrapper → the exported body
        // disappears too. Skipping alwaysinline keeps the body as
        // a standalone, exportable function.
        //
        // `contains("AzBoundary_")` matches both `AzBoundary_<hex>`
        // (when the wrapper is exported) and `__az_dep_AzBoundary_<hex>`
        // (when the wrapper is internal — boundary lift's preferred
        // shape; the wrapper gets gc-stripped after link).
        let patched_ir = if export_as.starts_with("AzStartup_")
            || export_as.contains("AzBoundary_")
        {
            lifted_ir.clone()
        } else {
            inject_alwaysinline(&lifted_ir, canonical_entry_addr)
        };
        let patched_ir_path = self.scratch_dir.join(format!("{}.patched.ll", stem));
        std::fs::write(&patched_ir_path, &patched_ir).map_err(|e| TranspileError {
            fn_name: fn_name.to_string(),
            reason: format!("write patched IR: {e}"),
        })?;

        // Wrapper signature + export name are caller-chosen. For
        // per-callback widget lifts (`lift_function`) the canonical
        // `Callback` shape exported as `callback`. For eventloop
        // lifts (M8.2 — `lift_eventloop_objects`) the caller picks
        // per AzStartup_<name>. The discovery side doesn't carry a
        // typedef tag through yet for widget callbacks; M7+ extends
        // `DiscoveredCallback` with one set at the attachment site
        // (set_on_toggle / set_on_value_change / layout_callback)
        // and the caller would route per kind via
        // `signature_for_callback_kind`.
        //
        // M8.8 Stage 1: parse `declare ptr @sub_<hex>` lines from the
        // post-rewrite IR. Each `<hex>` is now the symbol's canonical
        // address (PLT-chased, stub-shim-chased). The helper IR
        // generator looks up each address in the SymbolTable and
        // emits the body shape that matches the classification —
        // BumpAlloc bumps, CallIndirect bridges, ResolveCallback
        // bridges to JS, Leaf noops, Recursable gets no body (the
        // recursive walker provides it from a sibling .o).
        // M9-review: post-rewrite IR uses SYNTHETIC addresses in
        // `sub_<hex>` tokens. Look up classifications by synth
        // (slow O(n) linear scan; acceptable since this runs once
        // per lifted function and the branch list is small).
        let branch_sym_names = parse_extern_sub_declares(&lifted_ir);
        let mut resolved_branches: Vec<ResolvedBranchExtern> =
            Vec::with_capacity(branch_sym_names.len());
        for sym_name in &branch_sym_names {
            let synth_addr = parse_sub_hex_as_addr(sym_name).unwrap_or(0);
            let classification = symbol_table::get()
                .and_then(|t| t.lookup_by_synth(synth_addr))
                .map(|e| e.classification);
            eprintln!(
                "[azul-web]   intercept: {} → synth=0x{:x} class={:?}",
                sym_name, synth_addr, classification,
            );
            resolved_branches.push(ResolvedBranchExtern {
                sym_name: sym_name.clone(),
                classification,
            });
        }
        // Helper IR's entry-call references must use the canonical
        // entry address, NOT lift_addr — the lifted IR's defn after
        // rewrite is `define ptr @sub_<canonical_entry_addr_hex>`,
        // and the per-cb path uses a synthetic lift_addr of
        // 0x100000000 that wouldn't match.
        let helper_ir = emit_helper_ir(
            canonical_entry_addr,
            sig,
            &resolved_branches,
            export_as,
        );
        // M10-B1.a: tag the wrapper IR's prologue/return loads/stores
        // and the BumpAlloc/Realloc/Dealloc stub bodies' host accesses
        // with host metadata too. The memory intrinsics already carry
        // guest metadata from emission and are skipped by the tagger.
        let helper_ir = tag_state_accesses(&helper_ir);
        let helper_ir_path = self.scratch_dir.join(format!("{}.helper.ll", stem));
        std::fs::write(&helper_ir_path, &helper_ir).map_err(|e| TranspileError {
            fn_name: fn_name.to_string(),
            reason: format!("write helper IR: {e}"),
        })?;

        // Compile to wasm32 object.
        //
        // Native path (M8.9 Phase 2a, gated on AZ_NATIVE_REMILL=1):
        // concatenate patched_ir + helper_ir text-side (stripping
        // helper's `target datalayout` / `target triple` headers so
        // LLVM's parseIR doesn't see duplicates), then call
        // `az_remill_compile_to_wasm32_obj` which runs opt -O2 + llc
        // -mtriple=wasm32 in-process.
        //
        // Subprocess path: `llvm-link` merges patched_ir + helper_ir,
        // `opt -O2` cleans + inlines, `llc -mtriple=wasm32` emits the
        // .o. Three process spawns + intermediate file I/O.
        let obj_path = self.scratch_dir.join(format!("{}.o", stem));
        if use_native {
            #[cfg(feature = "web-transpiler-static")]
            {
                // Pass patched_ir + helper_ir as separate modules —
                // the C++ side runs llvm::Linker::linkInModule to
                // merge them, then opt -O2 + llc -mtriple=wasm32.
                // This matches `llvm-link` semantics; text concat
                // can't handle cross-module declare/define attribute
                // mismatches on `__remill_*`.
                let obj_bytes = super::native_remill::compile_to_wasm32_obj(&[
                    patched_ir.as_str(),
                    helper_ir.as_str(),
                ])
                .map_err(|e| TranspileError {
                    fn_name: fn_name.to_string(),
                    reason: format!("native compile: {}", e),
                })?;
                std::fs::write(&obj_path, &obj_bytes).map_err(|e| TranspileError {
                    fn_name: fn_name.to_string(),
                    reason: format!("write obj: {e}"),
                })?;
            }
        } else {
            let tools = tools.as_ref().expect("tools required for subprocess compile");
            let linked_ir_path = self.scratch_dir.join(format!("{}.linked.ll", stem));
            let llvm_link = self.llvm_link.as_deref().ok_or_else(|| TranspileError {
                fn_name: fn_name.to_string(),
                reason: "llvm-link not found — set $LLVM_LINK or install LLVM 21".into(),
            })?;
            run_tool(
                llvm_link,
                &[
                    "-S",
                    patched_ir_path.to_str().expect("scratch path is utf-8"),
                    helper_ir_path.to_str().expect("scratch path is utf-8"),
                    "-o",
                    linked_ir_path.to_str().expect("scratch path is utf-8"),
                ],
                fn_name,
            )?;

            let opt_ir_path = self.scratch_dir.join(format!("{}.opt.ll", stem));
            let opt = self.opt.as_deref().ok_or_else(|| TranspileError {
                fn_name: fn_name.to_string(),
                reason: "opt not found — set $LLVM_OPT or install LLVM 21".into(),
            })?;
            run_tool(
                opt,
                &[
                    opt_flag_for(fn_name),
                    "-S",
                    linked_ir_path.to_str().expect("scratch path is utf-8"),
                    "-o",
                    opt_ir_path.to_str().expect("scratch path is utf-8"),
                ],
                fn_name,
            )?;

            // M12.6 FIX (now DEFAULT; set AZ_NO_FIX_SP=1 to disable): enforce
            // SP + callee-saved (X19-X29) preservation across every lifted
            // `call sub_<hex>`. Repairs callees (e.g. CssProperty::clone, and
            // any `-> !`/early-exit path) whose lift drops the epilogue
            // `add sp,#N` and leaks the guest SP into the caller's frame —
            // which cumulatively drifts create_from's SP-relative cache base
            // toward NULL (the M12 node_count-corruption / 768 MiB OOB).
            if std::env::var_os("AZ_NO_FIX_SP").is_none() {
                if let Ok(opt_ir) = std::fs::read_to_string(&opt_ir_path) {
                    let (fixed, n) = enforce_sp_preservation(&opt_ir);
                    if n > 0 {
                        let _ = std::fs::write(&opt_ir_path, &fixed);
                    }
                }
            }

            // M12.7 (DEFAULT; AZ_NO_TRAP_SELFLOOP=1 to disable): rewrite empty
            // infinite self-loops (`LABEL:` then only `br label %LABEL`) into
            // `unreachable`. remill lifts `b .` / abort-spin instructions (and
            // opt can fold a loop's exit away) into a block that branches only
            // to itself, which HANGS the wasm (a 120 s gate timeout) instead of
            // trapping. In the lifted layout/cascade these are abort /
            // should-not-reach paths, so a trap is both correct and far faster
            // to debug than a hang. See `rewrite_empty_self_loops`.
            // M12.7 (AZ_LOG_SELFLOOP_VAL=<stem>|ALL diagnostic): before the
            // empty self-loops become traps, log the value `v` that routes INTO
            // each (the `icmp eq i64 %v, 0` operand) to 0x40078, so a post-trap
            // peek reveals WHAT non-zero value opt folded the loop-exit on.
            if let Ok(target) = std::env::var("AZ_LOG_SELFLOOP_VAL") {
                let matched = target == "ALL"
                    || target
                        .split(',')
                        .any(|s| !s.is_empty() && (fn_name.contains(s) || stem.contains(s)));
                if matched {
                    if let Ok(opt_ir) = std::fs::read_to_string(&opt_ir_path) {
                        let (logged, n) = inject_selfloop_value_log(&opt_ir);
                        if n > 0 {
                            let _ = std::fs::write(&opt_ir_path, &logged);
                            eprintln!(
                                "[azul-web] M12.7: logged {} self-loop routing values in {}",
                                n, stem
                            );
                        }
                    }
                }
            }
            if std::env::var_os("AZ_NO_TRAP_SELFLOOP").is_none() {
                if let Ok(opt_ir) = std::fs::read_to_string(&opt_ir_path) {
                    let (fixed, n) = rewrite_empty_self_loops(&opt_ir);
                    if n > 0 {
                        let _ = std::fs::write(&opt_ir_path, &fixed);
                    }
                }
            }

            // M12.5y store-address tracer: when AZ_LOG_STORES contains a
            // comma-separated substring matching this dep's stem, instrument
            // the post-opt IR in place (then llc compiles the instrumented
            // version). See `inject_store_logging`.
            if let Ok(target) = std::env::var("AZ_LOG_STORES") {
                let is_wrapper =
                    export_as.starts_with("AzStartup_") || export_as.contains("AzBoundary_");
                let matched = if target == "ALL" {
                    !is_wrapper
                } else {
                    target
                        .split(',')
                        .any(|s| !s.is_empty() && (fn_name.contains(s) || stem.contains(s)))
                };
                if matched {
                    // deptag = low 32 bits of the dep's runtime addr (the hex
                    // suffix of `__az_dep_<hex>`); 0 for non-dep export names.
                    let deptag = export_as
                        .rsplit('_')
                        .next()
                        .and_then(|h| u64::from_str_radix(h, 16).ok())
                        .map(|v| v as u32)
                        .unwrap_or(0);
                    if let Ok(opt_ir) = std::fs::read_to_string(&opt_ir_path) {
                        let (instrumented, n) = inject_store_logging(&opt_ir, deptag);
                        let _ = std::fs::write(&opt_ir_path, &instrumented);
                        let _ = std::fs::write(
                            self.scratch_dir.join(format!("{}.instr.ll", stem)),
                            &instrumented,
                        );
                        eprintln!(
                            "[azul-web] M12.5y: instrumented {} stores in {} (deptag=0x{:x})",
                            n, stem, deptag
                        );
                    }
                }
            }

            // M12.7 unreachable tagger: when AZ_TAG_UNREACHABLE matches this
            // fn's stem, tag each `unreachable` in the post-opt IR with a
            // unique id (a `store volatile 0x554e0000|id` to 0x40050) right
            // before it, so a post-trap peek of 0x40050 reveals WHICH
            // unreachable fired (the live opt-folded trap). Map id → the Nth
            // `unreachable` in the saved `.untag.ll`. See
            // `inject_unreachable_tagging`.
            if let Ok(target) = std::env::var("AZ_TAG_UNREACHABLE") {
                let matched = target == "ALL"
                    || target
                        .split(',')
                        .any(|s| !s.is_empty() && (fn_name.contains(s) || stem.contains(s)));
                if matched {
                    if let Ok(opt_ir) = std::fs::read_to_string(&opt_ir_path) {
                        let (tagged, n) = inject_unreachable_tagging(&opt_ir);
                        let _ = std::fs::write(&opt_ir_path, &tagged);
                        let _ = std::fs::write(
                            self.scratch_dir.join(format!("{}.untag.ll", stem)),
                            &tagged,
                        );
                        eprintln!(
                            "[azul-web] M12.7: tagged {} unreachables in {}",
                            n, stem
                        );
                    }
                }
            }

            // M12.7 loop-fuel hang-finder: when AZ_FUEL matches this fn's
            // stem (or "ALL"), instrument every terminator with a fuel tick
            // that traps after AZ_FUEL_LIMIT block-executions. Run with
            // AZ_WASM_DEBUG=1 so the trap's named stack pinpoints the
            // looping fn. See `inject_fuel`.
            if let Ok(target) = std::env::var("AZ_FUEL") {
                let is_wrapper =
                    export_as.starts_with("AzStartup_") || export_as.contains("AzBoundary_");
                let matched = (target == "ALL" && !is_wrapper)
                    || target
                        .split(',')
                        .any(|s| !s.is_empty() && (fn_name.contains(s) || stem.contains(s)));
                if matched {
                    if let Ok(opt_ir) = std::fs::read_to_string(&opt_ir_path) {
                        let (fueled, n) = inject_fuel(&opt_ir);
                        let _ = std::fs::write(&opt_ir_path, &fueled);
                        // Save for id->block mapping: the trap's 0x40070 = the
                        // Nth `call @__az_fuel(i32 N)` here = the looping block.
                        let _ = std::fs::write(
                            self.scratch_dir.join(format!("{}.fuel.ll", stem)),
                            &fueled,
                        );
                        eprintln!("[azul-web] M12.7: fueled {} terminators in {}", n, stem);
                    }
                }
            }

            run_tool(
                tools.llc,
                &[
                    "-mtriple=wasm32-unknown-unknown",
                    "-filetype=obj",
                    opt_flag_for(fn_name),
                    "-o",
                    obj_path.to_str().expect("scratch path is utf-8"),
                    opt_ir_path.to_str().expect("scratch path is utf-8"),
                ],
                fn_name,
            )?;
        }

        // produce_object_for stops here — caller decides whether to
        // wasm-ld this object alone or link several together.
        Ok(obj_path)
    }

    /// Default per-callback pipeline: lift a single function, then
    /// wasm-ld its `.o` into a self-contained `.wasm` module.
    /// Equivalent to the M5-M7 path.
    fn pipeline_single(
        &self,
        fn_name: &str,
        fn_addr: usize,
        fn_size: usize,
        sig: &CallbackSignature,
        export_as: &str,
    ) -> Result<WasmModule, TranspileError> {
        // Per-callback lifts use the historical fixed lift_addr
        // (matches the blueprint experiment; harmless because only
        // one object is linked).
        let lift_addr: u64 = 0x100000000;
        let obj_path = self.produce_object_for(
            fn_name, fn_addr, fn_size, sig, export_as, lift_addr,
        )?;
        let tools = self.tools(fn_name)?;
        let stem = sanitize_filename(fn_name);
        let wasm_path = self.scratch_dir.join(format!("{}.wasm", stem));
        let export_flag = format!("--export={}", export_as);
        run_tool(
            tools.wasm_ld,
            &[
                "--no-entry",
                &export_flag,
                "--allow-undefined",
                "-o",
                wasm_path.to_str().expect("scratch path is utf-8"),
                obj_path.to_str().expect("scratch path is utf-8"),
            ],
            fn_name,
        )?;
        let wasm_bytes = std::fs::read(&wasm_path).map_err(|e| TranspileError {
            fn_name: fn_name.to_string(),
            reason: format!("read {}: {e}", wasm_path.display()),
        })?;
        Ok(WasmModule {
            content_hash: super::fnv1a64_hex(&wasm_bytes),
            bytes: wasm_bytes,
            exports: vec![export_as.to_string()],
            // TODO(M7/WB1.3): scan the lifted IR for external `call`s
            // and surface them as imports from azul-mini.wasm. For
            // now, leave empty so the caller treats the module as
            // self-contained.
            imports_from_mini: Vec::new(),
            used_boundaries: Vec::new(),
        })
    }

    /// Run wasm-ld over a batch of `.o` files into one `.wasm` with
    /// the named exports. Used by [`lift_and_link_eventloop`] to
    /// build `azul-mini.wasm` from the per-AzStartup_* objects.
    ///
    /// Adds `--import-table` so the funcref table is imported from
    /// `env.__indirect_function_table` (JS-owned, sized + populated
    /// at instantiate-time with the per-callback WASMs' `callback`
    /// exports). Without this wasm-ld would emit an internal
    /// 1-element table that JS can't populate, defeating
    /// `__az_call_indirect`.
    fn link_objects_to_wasm(
        &self,
        objects: &[PathBuf],
        exports: &[String],
        output_stem: &str,
        memory_mode: MemoryMode,
        accessed_pages: &std::collections::HashSet<usize>,
        accessed_ranges: &std::collections::HashSet<(usize, usize)>,
    ) -> Result<Vec<u8>, TranspileError> {
        // M8.9 Phase 2b: native lld::wasm path when AZ_NATIVE_REMILL=1.
        // The C++ wrapper writes each obj to a per-call temp dir
        // internally (lld's API takes file paths, not memory buffers),
        // then reads the output wasm back into a heap buffer. From
        // here it's the same input/output shape as the subprocess
        // path — bytes in, bytes out.
        //
        // Initial memory: 128 MiB. Sized to absorb the
        // synthetic-address bands assigned by
        // `SymbolTable::assign_synthetic_addresses`:
        //
        //     [0          .. 64 KiB)   wasm stack zone (per-wasm
        //                              slot via `relocate_stack_*`)
        //     [64 KiB     .. ~1 MiB)   user-binary image band
        //     [~1 MiB     .. ~81 MiB)  libazul.dylib image band
        //                              (its __TEXT + __DATA span)
        //     [96 MiB     .. 128 MiB)  bump-heap zone (~32 MiB)
        //
        // libazul spans ~80 MiB at the synth level (text + cstring
        // + const + DATA combined). 128 MiB fits everything with
        // a 32 MiB heap headroom. JS can grow further at runtime
        // via `memory.grow` if a cb's bump-alloc demand exceeds
        // that.
        //
        // Earlier 1 GiB / 3 GiB experiments were workarounds for
        // the pre-synth lift baking 200+ MiB runtime addresses as
        // constants — see `M9_REVIEW_AND_OPTION_A.md`. The synth
        // scheme makes those addresses predictably small so
        // memory can shrink back to the order-of-magnitude that
        // actually reflects the image sizes involved.
        let initial_memory_bytes: u32 = 512 * 1024 * 1024;
        let import_memory = matches!(memory_mode, MemoryMode::ImportMemory);
        // import_table mirrors the subprocess `--import-table` flag —
        // funcref table is JS-owned (sized + populated with per-cb
        // wasm `callback` exports at instantiate-time). Both per-cb /
        // per-layout (ImportMemory) and azul-mini.wasm (OwnMemory)
        // need this for __az_call_indirect to work.
        let import_table = true;
        let debug_link = std::env::var_os("AZ_WASM_DEBUG").is_some();
        if self.use_native_remill() {
            #[cfg(feature = "web-transpiler-static")]
            {
                let mut obj_bytes: Vec<Vec<u8>> = Vec::with_capacity(objects.len());
                for p in objects {
                    let bytes = std::fs::read(p).map_err(|e| TranspileError {
                        fn_name: output_stem.to_string(),
                        reason: format!("read {}: {e}", p.display()),
                    })?;
                    obj_bytes.push(bytes);
                }
                let linked = super::native_remill::wasm_link(
                    &obj_bytes,
                    exports,
                    import_memory,
                    import_table,
                    initial_memory_bytes,
                )
                .map_err(|e| TranspileError {
                    fn_name: output_stem.to_string(),
                    reason: format!("native wasm_link: {}", e),
                })?;
                let mut final_wasm = if debug_link {
                    linked
                } else {
                    // Run wasm-opt -Oz on the linked output for the same
                    // size win the subprocess path gets below.
                    let pre_opt_path = self.scratch_dir
                        .join(format!("{}.pre-opt.wasm", output_stem));
                    let _ = std::fs::write(&pre_opt_path, &linked);
                    postprocess_wasm_opt(&pre_opt_path, output_stem).unwrap_or(linked)
                };
                relocate_stack_if_non_mini(&mut final_wasm, memory_mode, output_stem);
                inject_user_binary_data_segments(&mut final_wasm, accessed_pages, accessed_ranges, output_stem);
                return Ok(final_wasm);
            }
        }
        let tools = self.tools(output_stem)?;
        let wasm_path = self.scratch_dir.join(format!("{}.wasm", output_stem));
        // Set `AZ_WASM_DEBUG=1` to keep the names section + dwarf and
        // skip wasm-opt — lets `wasm-objdump` and stack traces show
        // the lifted-symbol names (`sub_<canonical_addr>`).
        let mut args: Vec<String> = vec![
            "--no-entry".to_string(),
            "--allow-undefined".to_string(),
            // --gc-sections strips unreachable functions (e.g. dead
            // lifted bodies the wrapper doesn't transitively call).
            // --strip-all removes debug/name/producer custom sections.
            // --lto-O2 enables cross-object LTO so dead code that
            // crosses .o boundaries also gets DCE'd.
            "--gc-sections".to_string(),
        ];
        if !debug_link {
            args.push("--strip-all".to_string());
            // M10-F2: --lto-O3 enables LTO with size-aware codegen.
            // wasm-ld passes the LTO opt level to LLVM; -O3 (not -Oz)
            // gives the best size in our measurements because LTO
            // can DCE cross-object dead globals + constants that
            // per-object opt -Oz already missed. Per-fn opt -Oz still
            // runs in C++ side via PassBuilder before this link step.
            args.push("--lto-O3".to_string());
        } else {
            // --keep-section preserves the function-names custom
            // section so wasm-objdump can map indices → `sub_<addr>`.
            args.push("--lto-O0".to_string());
            args.push("--keep-section=name".to_string());
        }
        if import_table {
            args.push("--import-table".to_string());
        }
        args.push("-o".to_string());
        args.push(wasm_path.to_string_lossy().into_owned());
        if import_memory {
            // Per-cb / per-layout wasms import `env.memory` so they
            // share linear address space with the mini wasm (which
            // exports `memory`). JS wires
            // `env.memory = mini.exports.memory` at instantiate time.
            // Without this each wasm has its own unconnected memory
            // and any pointer the caller passes in references the
            // wrong heap.
            args.push("--import-memory".to_string());
        }
        args.push(format!("--initial-memory={}", initial_memory_bytes));
        for e in exports {
            args.push(format!("--export={}", e));
        }
        for p in objects {
            args.push(p.to_string_lossy().into_owned());
        }
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run_tool(tools.wasm_ld, &arg_refs, output_stem)?;
        // Post-process with binaryen wasm-opt -Oz when available.
        // wasm-ld already runs --gc-sections + --strip-all + --lto-O2,
        // but wasm-opt's `-Oz` passes (local-cse, vacuum, simplify-
        // locals, merge-blocks, ...) shave another 10-20% on lifted
        // wasm. Best-effort: if wasm-opt isn't installed or fails,
        // serve the un-opt'd wasm.
        let opt_bytes = if debug_link {
            None
        } else {
            postprocess_wasm_opt(&wasm_path, output_stem)
        };
        let mut final_wasm = match opt_bytes {
            Some(b) => b,
            None => std::fs::read(&wasm_path).map_err(|e| TranspileError {
                fn_name: output_stem.to_string(),
                reason: format!("read {}: {e}", wasm_path.display()),
            })?,
        };
        relocate_stack_if_non_mini(&mut final_wasm, memory_mode, output_stem);
        inject_user_binary_data_segments(&mut final_wasm, accessed_pages, accessed_ranges, output_stem);
        Ok(final_wasm)
    }

    /// Recursively lift a set of root functions + every dependency
    /// they transitively reach. Stops at known-leaf classifications
    /// + at already-visited native addresses.
    ///
    /// **Symbol-naming**: each function is lifted at
    /// `lift_addr = native_addr`. Caller's `bl <native_target>`
    /// lifts to `call sub_<low_32_of_native_target>`, which
    /// matches the callee's defn `define sub_<low_32_of_native_addr>`
    /// — no rewriting needed.
    ///
    /// **Roots vs deps**: each root gets a wrapper exported under
    /// `export_as` (per its CallbackSignature). Deps recursively
    /// reached get a wrapper too (cheap to leave; wasm-ld's
    /// `--gc-sections` strips the unused ones), but their callable
    /// surface is the lifted body `sub_<addr_hex>` which other
    /// lifted code calls by name.
    ///
    /// **Depth limit**: hard-capped at `opts.max_recursive_depth`
    /// (default `LiftOpts::default()` = 256). If hit, returns an
    /// error so a runaway chain doesn't lock up the server forever.
    ///
    /// **Skipped externs**: dladdr fallbacks (`cb_<hex>` names),
    /// known-leaf classifications (RustAlloc, AzCallIndirect, etc.)
    /// don't recurse — they get bodies from helper IR. Unresolved
    /// externs become noop stubs.
    pub fn lift_with_transitive_deps(
        &self,
        roots: Vec<TransitiveLiftRoot>,
    ) -> Result<WasmModule, TranspileError> {
        self.lift_with_transitive_deps_ex(roots, LiftOpts::default())
    }

    /// Like [`Self::lift_with_transitive_deps`] but exposes per-call
    /// link options (extra prelinked objects, memory mode, output
    /// stem, recursion cap). M11 Sprint 1 added this so the
    /// eventloop pipeline can route through the transitive lifter
    /// while still bundling `bump_helpers.o` + owning its own wasm
    /// memory.
    pub fn lift_with_transitive_deps_ex(
        &self,
        roots: Vec<TransitiveLiftRoot>,
        opts: LiftOpts,
    ) -> Result<WasmModule, TranspileError> {
        // M8.9 Phase 3b: in the native pipeline, pre-walk the dep
        // graph via ARM64 bytes-scan to discover the full set
        // upfront, then batch-lift everything in one call. Saves
        // (N-1)×LoadArchSemantics cost (~30 ms each) — for the
        // hello-world transitive on_click (~12 fns) that's ~330 ms
        // off the first request.
        if self.use_native_remill() {
            #[cfg(feature = "web-transpiler-static")]
            return self.lift_with_transitive_deps_batched(roots, &opts);
        }
        self.lift_with_transitive_deps_sequential(roots, &opts)
    }

    fn lift_with_transitive_deps_sequential(
        &self,
        roots: Vec<TransitiveLiftRoot>,
        opts: &LiftOpts,
    ) -> Result<WasmModule, TranspileError> {
        // Hard cap on the number of functions a single root's
        // transitive closure can pull in. Bumped from 64 → 256 in
        // M8.8 once exact-size lifts surface the full layout-cb
        // dependency graph (40+ azul-css / azul-core / azul-layout
        // helpers around CssVec/DomVec/Dom clones+drops). With the
        // per-canonical-addr `object_cache` below, repeat lifts of
        // the same dep across multiple callbacks are O(1), so the
        // cap is about call-graph fan-out, not throughput.
        //
        // M11 Sprint 1: caller-tunable via `LiftOpts::max_recursive_depth`
        // — eventloop bumps this to absorb cascade + layout deps.
        let max_recursive_depth = opts.max_recursive_depth;

        let mut visited: HashSet<usize> = HashSet::new();
        let mut queue: VecDeque<TransitiveLiftTarget> = VecDeque::new();
        let mut object_paths: Vec<PathBuf> = Vec::new();
        let mut exports: Vec<String> = Vec::new();
        // M9-after-review: collect ARM64 `adrp` page targets across
        // every lifted function so the data mirror only ships the
        // pages this wasm actually reads — not the entire 19 MiB
        // libazul `__const` blob.
        let mut accessed_pages: HashSet<usize> = HashSet::new();
        // M10-E1: precise (native_addr, len) byte ranges harvested
        // from adrp+ldr pairs — used to ship only the exact bytes
        // each lifted load reads, not the whole page.
        let mut accessed_ranges: HashSet<(usize, usize)> = HashSet::new();
        // M10-D: boundary canonical addrs surfaced during the BFS.
        // Returned via `WasmModule.used_boundaries`; orchestrator
        // unions across every lift + runs the boundary-lift pass.
        let mut used_boundaries: HashSet<usize> = HashSet::new();
        // M10-D: extra exports per root, flattened — appended to
        // wasm-ld's `--export` list so the boundary lift can expose
        // its raw `sub_<canonical_hex>` body alongside the wrapper.
        let mut extra_exports: Vec<String> = Vec::new();
        for r in &roots {
            extra_exports.extend(r.extra_exports.iter().cloned());
        }

        for root in roots {
            queue.push_back(TransitiveLiftTarget::Root(root));
        }

        let canonical_sig = signature_for_callback_kind("Callback");
        let mut lifted_count = 0usize;

        while let Some(target) = queue.pop_front() {
            let (name, addr, size, sig, export_as) = match target {
                TransitiveLiftTarget::Root(r) => {
                    (r.fn_name, r.fn_addr, r.fn_size, r.sig, r.export_as)
                }
                TransitiveLiftTarget::Dep { name, addr, size } => (
                    name,
                    addr,
                    size,
                    canonical_sig.clone(),
                    format!("__az_dep_{:x}", addr),
                ),
            };

            if !visited.insert(addr) {
                continue;
            }
            lifted_count += 1;
            if lifted_count > max_recursive_depth {
                return Err(TranspileError {
                    fn_name: name,
                    reason: format!(
                        "transitive lift exceeded {} functions — runaway recursion?",
                        max_recursive_depth
                    ),
                });
            }

            // Harvest adrp pages from this fn's bytes before lift.
            let fn_bytes_slice = unsafe {
                std::slice::from_raw_parts(addr as *const u8, size)
            };
            for page in scan_arm64_adrp_pages(fn_bytes_slice, addr) {
                accessed_pages.insert(page);
            }
            // M10-E1: also harvest exact (addr, len) byte ranges
            // from adrp+ldr pairs so the mirror can ship only the
            // bytes the lifted code actually reads.
            for r in scan_arm64_adrp_accesses(fn_bytes_slice, addr) {
                accessed_ranges.insert(r);
            }

            // M9-review: pass the per-image synthetic address as
            // `--address=` so the lifted IR's `adrp+ldr` page targets
            // land in wasm-friendly low offsets. Symbol resolution
            // continues to work because per-image distances are
            // preserved in synthetic space.
            let lift_addr = symbol_table::get()
                .and_then(|t| t.lookup(addr))
                .map(|e| e.synthetic_addr as u64)
                .unwrap_or(addr as u64);

            // M8.8 perf: check the per-(addr, export_as) cache before
            // running the (expensive) remill+opt+llc+llvm-link
            // subprocess chain. Cross-callback dep sharing falls out
            // for free — layout cb and on_click both pulling in
            // AzRefCount_clone now reuse one .o instead of producing
            // two identical ones.
            let cache_key = (addr, export_as.clone());
            let cached = self
                .object_cache
                .lock()
                .unwrap()
                .get(&cache_key)
                .cloned();
            let obj = match cached {
                Some(p) => {
                    eprintln!(
                        "[azul-web]   transitive[{}]: cached {} addr=0x{:016x} → {}",
                        lifted_count,
                        name,
                        addr,
                        p.display()
                    );
                    p
                }
                None => {
                    eprintln!(
                        "[azul-web]   transitive[{}]: lifting {} addr=0x{:016x} \
                         size={} export_as={}",
                        lifted_count, name, addr, size, export_as
                    );
                    let produced = self.produce_object_for(
                        &name, addr, size, &sig, &export_as, lift_addr,
                    )?;
                    self.object_cache
                        .lock()
                        .unwrap()
                        .insert(cache_key, produced.clone());
                    produced
                }
            };
            object_paths.push(obj);
            exports.push(export_as);

            // Parse this lift's branch externs + enqueue deps.
            let stem = sanitize_filename(&name);
            let lifted_ir_path = self.scratch_dir.join(format!("{}.lifted.ll", stem));
            let lifted_ir = match std::fs::read_to_string(&lifted_ir_path) {
                Ok(s) => s,
                Err(_) => {
                    // remill should always produce the .lifted.ll
                    // — if it's missing, something upstream went
                    // wrong; just continue without enqueueing deps.
                    continue;
                }
            };
            // M8.8 Stage 1: walk dep call sites via the post-rewrite
            // IR. Each `sub_<hex>` is the dep's canonical address
            // (PLT-chased, shim-chased) — no per-call lift-space
            // arithmetic. SymbolTable classification drives the
            // "recurse-or-skip" decision; only Recursable symbols
            // get queued.
            let rewritten_for_walk = match symbol_table::get() {
                Some(table) => {
                    rewrite_sub_names_to_canonical(&lifted_ir, table, addr, lift_addr)
                }
                None => lifted_ir.clone(),
            };
            for sym in parse_extern_sub_declares(&rewritten_for_walk) {
                let Some(canonical_synth) = parse_sub_hex_as_addr(&sym) else {
                    eprintln!("[azul-web]     dep: {} (canonical hex parse failed)", sym);
                    continue;
                };
                // M9-review: post-rewrite IR uses synth addrs. Look up
                // by synth, then queue the dep's NATIVE canonical_addr
                // for the lift loop (the lift itself rebases).
                let entry = match symbol_table::get()
                    .and_then(|t| t.lookup_by_synth(canonical_synth))
                {
                    Some(e) => e.clone(),
                    None => {
                        eprintln!(
                            "[azul-web]     dep: {} synth=0x{:x} not in SymbolTable — skipping",
                            sym, canonical_synth,
                        );
                        continue;
                    }
                };
                let already_visited = visited.contains(&entry.canonical_addr);
                eprintln!(
                    "[azul-web]     dep: {} → resolved={}@0x{:016x} class={:?} visited={}  (pulled in by {})",
                    sym,
                    entry.canonical_name,
                    entry.canonical_addr,
                    entry.classification,
                    already_visited,
                    name,
                );
                if already_visited {
                    continue;
                }
                // M10-D: record boundaries before the recursable
                // check so they're tracked even though they don't
                // recurse. Boundaries become env-imports in the
                // cb wasm and ship as separate per-fn shards.
                if entry.classification.is_boundary_import() {
                    used_boundaries.insert(entry.canonical_addr);
                    continue;
                }
                if !entry.classification.is_recursable() {
                    // Leaf / BumpAlloc / CallIndirect /
                    // ResolveCallback / NeverLift: the helper IR
                    // emits the right body. Don't recurse into the
                    // dep's own call graph.
                    continue;
                }
                queue.push_back(TransitiveLiftTarget::Dep {
                    name: entry.canonical_name.clone(),
                    addr: entry.canonical_addr,
                    size: if entry.size > 0 {
                        entry.size
                    } else {
                        super::LIFT_READ_WINDOW
                    },
                });
            }
        }

        // M10-D: append per-root extra exports (boundary-lift's
        // `sub_<canonical_hex>` body export). Dedup so we don't pass
        // `--export` twice for a name already in the wrapper-export
        // list.
        for extra in &extra_exports {
            if !exports.iter().any(|e| e == extra) {
                exports.push(extra.clone());
            }
        }
        // M11 Sprint 1: append caller-supplied passthrough exports
        // (e.g. eventloop's bump_helper exports defined in extra_objects).
        for extra in &opts.extra_exports_passthrough {
            if !exports.iter().any(|e| e == extra) {
                exports.push(extra.clone());
            }
        }
        // M11 Sprint 1: append caller-supplied prelinked objects
        // (e.g. eventloop's bump_helpers.o).
        for obj in &opts.extra_objects {
            object_paths.push(obj.clone());
        }

        eprintln!(
            "[azul-web] transitive lift complete: {} functions lifted, {} unique exports",
            visited.len(),
            exports.len()
        );

        let bytes = self.link_objects_to_wasm(
            &object_paths,
            &exports,
            &opts.output_stem,
            opts.memory_mode,
            &accessed_pages,
            &accessed_ranges,
        )?;
        let mut boundaries: Vec<usize> = used_boundaries.into_iter().collect();
        boundaries.sort_unstable();
        if !boundaries.is_empty() {
            eprintln!(
                "[azul-web] transitive lift (sequential): used {} boundary imports",
                boundaries.len(),
            );
        }
        Ok(WasmModule {
            content_hash: super::fnv1a64_hex(&bytes),
            bytes,
            exports,
            imports_from_mini: Vec::new(),
            used_boundaries: boundaries,
        })
    }

    /// Native-only batched transitive lift. Pre-walks the dep graph
    /// via ARM64 bytes-scan to discover the full set upfront, then
    /// calls `native_remill::lift_batch` once to lift everything in
    /// one LoadArchSemantics-amortized pass. Each per-fn IR feeds
    /// `produce_object_from_lifted_ir`; the resulting .o set links
    /// to the per-cb wasm via the existing `link_objects_to_wasm`.
    #[cfg(feature = "web-transpiler-static")]
    fn lift_with_transitive_deps_batched(
        &self,
        roots: Vec<TransitiveLiftRoot>,
        opts: &LiftOpts,
    ) -> Result<WasmModule, TranspileError> {
        // M11 Sprint 1: caller-tunable via `LiftOpts::max_recursive_depth`
        // — eventloop bumps this to absorb cascade + layout deps.
        let max_recursive_depth = opts.max_recursive_depth;
        // Per-target metadata for the lift + post-process pipeline.
        struct Target {
            name: String,
            addr: usize,
            size: usize,
            sig: CallbackSignature,
            export_as: String,
        }

        let canonical_sig = signature_for_callback_kind("Callback");
        let table = symbol_table::get();

        // BFS pre-walk via bytes-scan. Builds the deduplicated set
        // of (name, addr, size, sig, export_as) without any lift.
        let mut visited: HashSet<usize> = HashSet::new();
        let mut targets: Vec<Target> = Vec::new();
        // M10-D: track every BoundaryImport canonical addr reached
        // during the BFS — the orchestrator will lift each into its
        // own per-fn wasm shard. Empty in legacy bundled mode.
        let mut used_boundaries: HashSet<usize> = HashSet::new();
        // M10-E1: precise (native_addr, len) byte ranges from
        // adrp+ldr pair scanning across every target's bytes.
        let mut accessed_ranges: HashSet<(usize, usize)> = HashSet::new();
        // M10-D: extra exports per root, flattened into one Vec.
        // Appended verbatim to the final wasm-ld --export list so
        // the boundary lift can expose its raw `sub_<canonical_hex>`
        // body alongside the wrapper.
        let mut extra_exports: Vec<String> = Vec::new();
        for r in &roots {
            extra_exports.extend(r.extra_exports.iter().cloned());
        }
        let mut queue: VecDeque<(String, usize, usize, CallbackSignature, String)> =
            VecDeque::new();
        for r in roots {
            // Canonicalize the root's fn_addr too — if the user-supplied
            // addr is a PLT stub or bare-`b` tail-shim, resolve() chases
            // through to the real callee. Without this, a dep elsewhere
            // in the graph that lands on the canonical addr produces a
            // .o whose `define ptr @sub_<canonical_hex>` collides with
            // the root's .o (the rewrite step renames every lifted
            // `sub_<X>` to `sub_<canonical_X>`). wasm-ld then errors on
            // duplicate symbol.
            let (canonical_addr, canonical_size) = match table.and_then(|t| t.resolve(r.fn_addr)) {
                Some(entry) => (
                    entry.canonical_addr,
                    if entry.size > 0 { entry.size } else { r.fn_size },
                ),
                None => (r.fn_addr, r.fn_size),
            };
            queue.push_back((r.fn_name, canonical_addr, canonical_size, r.sig, r.export_as));
        }
        while let Some((name, addr, size, sig, export_as)) = queue.pop_front() {
            if !visited.insert(addr) {
                continue;
            }
            if targets.len() >= max_recursive_depth {
                return Err(TranspileError {
                    fn_name: name,
                    reason: format!(
                        "transitive lift exceeded {} functions — runaway recursion?",
                        max_recursive_depth
                    ),
                });
            }
            // Scan this fn's bytes for BL/B targets.
            let bytes_slice = unsafe { std::slice::from_raw_parts(addr as *const u8, size) };
            let bl_targets = scan_arm64_bl_b_targets(bytes_slice, addr);
            targets.push(Target {
                name: name.clone(),
                addr,
                size,
                sig,
                export_as,
            });
            // Enqueue recursable deps. Use `resolve()` (not `lookup()`)
            // so PLT-stub / bare-`b` tail-shim addresses chase through
            // to the real callee — matches the IR-walk path which
            // operates on canonical-rewritten names.
            let Some(table) = table else { continue; };
            for dep_addr in bl_targets {
                let Some(entry) = table.resolve(dep_addr) else { continue; };
                // M10-D: record every BoundaryImport reached during the
                // BFS so the orchestrator can lift it into a per-fn
                // shard. Don't enqueue — boundaries don't bundle their
                // body into this wasm; the cb's wasm-ld run will leave
                // `sub_<canonical_hex>` undefined, and `--allow-undefined`
                // converts it into an env-import wired by loader.js.
                if entry.classification.is_boundary_import() {
                    used_boundaries.insert(entry.canonical_addr);
                    continue;
                }
                if !entry.classification.is_recursable() {
                    continue;
                }
                if visited.contains(&entry.canonical_addr) {
                    continue;
                }
                let dep_size = if entry.size > 0 {
                    entry.size
                } else {
                    super::LIFT_READ_WINDOW
                };
                queue.push_back((
                    entry.canonical_name.clone(),
                    entry.canonical_addr,
                    dep_size,
                    canonical_sig.clone(),
                    format!("__az_dep_{:x}", entry.canonical_addr),
                ));
            }
        }

        eprintln!(
            "[azul-web]   transitive (batched): pre-walk discovered {} fns",
            targets.len(),
        );

        // Debug: verify all targets have unique addr — a duplicate
        // would explain "wasm-ld: error: duplicate symbol: sub_<X>"
        // (two .o files defining sub_<canonical_X>).
        if std::env::var_os("AZ_REMILL_DEBUG").is_some() {
            let mut seen_addrs: HashSet<usize> = HashSet::new();
            for t in &targets {
                if !seen_addrs.insert(t.addr) {
                    eprintln!(
                        "[az_remill_debug] DUPLICATE addr in targets: {} (export_as={}) — \
                         would produce duplicate sub_<{:x}>",
                        t.name, t.export_as, t.addr,
                    );
                }
            }
        }

        // M9-after-review: collect ARM64 `adrp` page targets across
        // every target's bytes — even for cached objects we still
        // need their accessed pages in the mirror set.
        let mut accessed_pages: HashSet<usize> = HashSet::new();
        for t in &targets {
            let bytes_slice = unsafe {
                std::slice::from_raw_parts(t.addr as *const u8, t.size)
            };
            for page in scan_arm64_adrp_pages(bytes_slice, t.addr) {
                accessed_pages.insert(page);
            }
            // M10-E1: precise byte-range scan for adrp+ldr pairs.
            for r in scan_arm64_adrp_accesses(bytes_slice, t.addr) {
                accessed_ranges.insert(r);
            }
        }

        // Cache check: split into (cached, to_lift) per target.
        let mut object_paths: Vec<PathBuf> = Vec::with_capacity(targets.len());
        let mut exports: Vec<String> = Vec::with_capacity(targets.len());
        let mut to_lift_idx: Vec<usize> = Vec::new();
        for (i, t) in targets.iter().enumerate() {
            let key = (t.addr, t.export_as.clone());
            let cached = self.object_cache.lock().unwrap().get(&key).cloned();
            if let Some(p) = cached {
                eprintln!(
                    "[azul-web]   transitive[{}/{}]: cached {} addr=0x{:016x}",
                    i + 1,
                    targets.len(),
                    t.name,
                    t.addr,
                );
                object_paths.push(p);
                exports.push(t.export_as.clone());
            } else {
                to_lift_idx.push(i);
            }
        }

        if !to_lift_idx.is_empty() {
            let arch_tag = host_arch_tag().ok_or_else(|| TranspileError {
                fn_name: "transitive-batched".into(),
                reason: "unsupported host architecture".into(),
            })?;
            let bytes_vec: Vec<Vec<u8>> = to_lift_idx
                .iter()
                .map(|&i| {
                    let t = &targets[i];
                    let mut v = unsafe {
                        std::slice::from_raw_parts(t.addr as *const u8, t.size).to_vec()
                    };
                    rewrite_ldapr_to_ldar(&mut v);
                    rewrite_recursive_bl(&mut v);
                    v
                })
                .collect();
            // M9-review: lift with synthetic addresses so the IR's
            // `adrp+ldr` page targets land in wasm-friendly low
            // offsets. Native `t.addr` is now used only as the
            // identity key for caching + symbol resolution.
            let synth_of = |native_addr: usize| -> u64 {
                symbol_table::get()
                    .and_then(|t| t.lookup(native_addr))
                    .map(|e| e.synthetic_addr as u64)
                    .unwrap_or(native_addr as u64)
            };
            let items: Vec<(u64, &[u8])> = to_lift_idx
                .iter()
                .zip(bytes_vec.iter())
                .map(|(&i, b)| (synth_of(targets[i].addr), b.as_slice()))
                .collect();
            let t0 = std::time::Instant::now();
            let per_fn_irs = super::native_remill::lift_batch(
                arch_tag,
                host_os_tag(),
                &items,
            )
            .map_err(|e| TranspileError {
                fn_name: "transitive-batched".into(),
                reason: format!("native batched lift: {}", e),
            })?;
            eprintln!(
                "[azul-web]   transitive (batched): lifted {} items in {:?}",
                to_lift_idx.len(),
                t0.elapsed(),
            );

            // M10-B1.b: opt-in merged compile. Set
            // AZ_REMILL_MERGED_COMPILE=1 to run all per-fn IRs through
            // a single linkInModule + opt -O2 pass with alwaysinline
            // on every sub_<hex> define. Lets opt inline the entire
            // dep call graph into the entry wrapper → State alloca
            // SROA fires → layout.wasm shrinks dramatically.
            //
            // Default path (per-fn .o + wasm-ld) is unchanged so this
            // ships dormant and the cache stays usable for previous
            // session runs.
            // M10-E2 auto-merge: enable merged compile for SMALL
            // dep sets (cb wasms with ≤ MERGED_AUTO_THRESHOLD fns).
            // Small bodies inline cleanly into the wrapper, SROA
            // promotes the State alloca, ~70% wasm size reduction.
            // Large bodies (e.g. the full layout cb's 141 deps)
            // regress under merged mode — opt -O2's natural inliner
            // bloats the merged module faster than DCE recovers, so
            // we keep them on the per-fn .o path.
            //
            // Explicit env knobs still win:
            //   AZ_REMILL_MERGED_COMPILE=1 forces ON regardless of size.
            //   AZ_REMILL_DISABLE_AUTO_MERGE=1 forces OFF (regression-test path).
            const MERGED_AUTO_THRESHOLD: usize = 30;
            let env_force_on = std::env::var_os("AZ_REMILL_MERGED_COMPILE").is_some();
            let env_force_off = std::env::var_os("AZ_REMILL_DISABLE_AUTO_MERGE").is_some();
            let auto_on = targets.len() <= MERGED_AUTO_THRESHOLD;
            let merged_mode = self.use_native_remill()
                && !env_force_off
                && (env_force_on || auto_on);
            if merged_mode {
                let merge_t0 = std::time::Instant::now();
                let mut ir_pairs: Vec<(String, String)> =
                    Vec::with_capacity(to_lift_idx.len());
                // tag_with_alwaysinline_all=true crashes on dep graphs
                // with recursion cycles (alwaysinline + cycle is a hard
                // LLVM assert). For small dep sets the call graph is
                // typically a DAG and the win is dramatic; we auto-
                // enable for cbs with ≤ MERGED_AUTO_THRESHOLD fns.
                // Set AZ_REMILL_MERGED_ALWAYSINLINE=1 to force.
                let alwaysinline_all = auto_on
                    || std::env::var_os("AZ_REMILL_MERGED_ALWAYSINLINE").is_some();
                eprintln!(
                    "[azul-web]   merged-mode: {} fns (alwaysinline_all={})",
                    targets.len(),
                    alwaysinline_all,
                );
                for (&i, lifted_ir) in to_lift_idx.iter().zip(per_fn_irs.iter()) {
                    let t = &targets[i];
                    let lift_addr = synth_of(t.addr);
                    let (patched, helper) = self.prepare_per_fn_irs(
                        &t.name,
                        t.addr,
                        lift_addr,
                        &t.sig,
                        &t.export_as,
                        lifted_ir,
                        alwaysinline_all,
                    )?;
                    ir_pairs.push((patched, helper));
                }
                let merged_obj = self.compile_merged_transitive_object(&ir_pairs)?;
                eprintln!(
                    "[azul-web]   transitive (merged): compiled {} fns into one .o in {:?}",
                    to_lift_idx.len(),
                    merge_t0.elapsed(),
                );
                object_paths.push(merged_obj);
                // Only export the root entry (first target). Dep
                // wrappers exist in the merged .o but stay internal —
                // wasm-ld's `--gc-sections` strips them if not exported.
                // The roots come from `roots` which was canonicalized
                // into `targets[0..roots.len()]` order above.
                for &i in &to_lift_idx {
                    let t = &targets[i];
                    // Only export functions whose export_as does NOT
                    // start with `__az_dep_` (those are transitive
                    // deps the lift discovered, not user-callable
                    // entries).
                    if !t.export_as.starts_with("__az_dep_") {
                        exports.push(t.export_as.clone());
                    }
                }
            } else {
                for (&i, lifted_ir) in to_lift_idx.iter().zip(per_fn_irs.iter()) {
                    let t = &targets[i];
                    let lift_addr = synth_of(t.addr);
                    let obj = self.produce_object_from_lifted_ir(
                        &t.name, t.addr, lift_addr, &t.sig, &t.export_as, lifted_ir,
                    )?;
                    self.object_cache
                        .lock()
                        .unwrap()
                        .insert((t.addr, t.export_as.clone()), obj.clone());
                    object_paths.push(obj);
                    // M10-D-prep: only export the actual entry points
                    // (cb / layout). Dep wrappers (`__az_dep_<addr>`)
                    // are never called from JS — they're an artifact
                    // of emit_helper_ir always generating a wrapper
                    // per per-fn .o. Without exporting them,
                    // wasm-ld's --gc-sections strips them.
                    if !t.export_as.starts_with("__az_dep_") {
                        exports.push(t.export_as.clone());
                    }
                }
            }
        }

        // M10-D: append per-root extra exports (boundary-lift's
        // `sub_<canonical_hex>` body export). Dedup so we don't pass
        // `--export` twice for a name already in the wrapper-export
        // list.
        for extra in &extra_exports {
            if !exports.iter().any(|e| e == extra) {
                exports.push(extra.clone());
            }
        }
        // M11 Sprint 1: append caller-supplied passthrough exports
        // (e.g. eventloop's bump_helper exports defined in extra_objects).
        for extra in &opts.extra_exports_passthrough {
            if !exports.iter().any(|e| e == extra) {
                exports.push(extra.clone());
            }
        }
        // M11 Sprint 1: append caller-supplied prelinked objects
        // (e.g. eventloop's bump_helpers.o).
        for obj in &opts.extra_objects {
            object_paths.push(obj.clone());
        }

        eprintln!(
            "[azul-web] transitive lift complete (batched): {} functions, {} unique exports",
            targets.len(),
            exports.len(),
        );

        // Debug: log any duplicate paths (same .o file in object_paths
        // twice would explain a wasm-ld "duplicate symbol" error).
        if std::env::var_os("AZ_REMILL_DEBUG").is_some() {
            let mut seen: HashSet<PathBuf> = HashSet::new();
            for p in &object_paths {
                if !seen.insert(p.clone()) {
                    eprintln!("[az_remill_debug] DUPLICATE .o path in link: {}", p.display());
                }
            }
            eprintln!(
                "[az_remill_debug] link: {} paths, {} unique",
                object_paths.len(),
                seen.len(),
            );
        }

        let bytes = self.link_objects_to_wasm(
            &object_paths,
            &exports,
            &opts.output_stem,
            opts.memory_mode,
            &accessed_pages,
            &accessed_ranges,
        )?;
        let mut boundaries: Vec<usize> = used_boundaries.into_iter().collect();
        boundaries.sort_unstable();
        if !boundaries.is_empty() {
            eprintln!(
                "[azul-web] transitive lift (batched): used {} boundary imports",
                boundaries.len(),
            );
        }
        Ok(WasmModule {
            content_hash: super::fnv1a64_hex(&bytes),
            bytes,
            exports,
            imports_from_mini: Vec::new(),
            used_boundaries: boundaries,
        })
    }
}

impl Drop for RemillTranspiler {
    fn drop(&mut self) {
        // Wipe the scratch dir on drop. Each lift cycle writes 4-6
        // .ll files + 1 .o per fn (146 fns for a layout-cb workload
        // ≈ 800 files ≈ 18 MiB). Without this, server restarts
        // accumulate ~18 MiB/process forever in $TMPDIR.
        if RemillTranspiler::should_keep_scratch() {
            eprintln!(
                "[azul-web] RemillTranspiler drop: keeping scratch dir {} (AZ_REMILL_KEEP_SCRATCH=1)",
                self.scratch_dir.display(),
            );
            return;
        }
        if self.scratch_dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&self.scratch_dir) {
                eprintln!(
                    "[azul-web] RemillTranspiler drop: failed to wipe scratch dir {}: {}",
                    self.scratch_dir.display(),
                    e,
                );
            }
        }
    }
}

/// Counter handing out unique stack-base offsets to each non-mini wasm.
/// Each call to [`relocate_stack_if_non_mini`] bumps it; the new SP =
/// `STACK_BASE_OFFSET + slot * STACK_BASE_STRIDE`.
///
/// **Why this exists (M9-3)**: wasm-ld places each module's stack at
/// the very bottom of linear memory by default (stack-pointer global
/// initialised just above `__data_end`, typically ~64 KiB). All
/// per-cb / per-layout wasms share the *same* linear memory as mini,
/// so their stacks collide — when mini's lifted code calls into the
/// layout cb via `__az_call_indirect_layout4`, the layout cb's
/// wrapper writes a fresh 1088-byte `%state_buf` over what mini left
/// on the stack. Mini reads the (now zeroed) state on return and
/// traps on a deref.
///
/// Each non-mini wasm gets a unique stack region above mini's stack
/// but below the bump heap (which starts at 1 MiB). 128 KiB per slot
/// covers the 64 KiB stack + slack.
static NEXT_NON_MINI_STACK_SLOT: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);

const STACK_BASE_FIRST: u32 = 192 * 1024;   // 192 KiB
const STACK_BASE_STRIDE: u32 = 128 * 1024;  // 128 KiB / wasm

/// Patch the stack-pointer global (global[0]) of every wasm (both
/// mini and per-cb / per-layout) so each gets a distinct
/// non-overlapping stack region. Mini owns slot 0, every subsequent
/// link bumps the counter.
///
/// **Why mini moves too (M9-3)**: the M9-side fix for user-binary
/// const-string loads (a follow-up; see TaskList) mirrors the user
/// binary's __cstring / __DATA into wasm memory at the *truncated
/// low-32 native addresses* — typically `[0..64 KiB]`. wasm-ld's
/// default stack placement collides with that region (stack ends
/// at ~64 KiB). Moving every stack above 192 KiB clears the path
/// for the data mirror without breaking anything that needs the
/// stack to be in a specific place (nothing does — wasm stack
/// placement is determined entirely by global[0]'s init).
///
/// Returns silently on any wasm-format mismatch — best-effort. If
/// patching fails the wasm still loads + works for self-contained
/// scenarios; cross-module calls into the affected module will hit
/// the stack-overlap trap.
fn relocate_stack_if_non_mini(wasm: &mut Vec<u8>, memory_mode: MemoryMode, output_stem: &str) {
    let _ = memory_mode;
    let slot = NEXT_NON_MINI_STACK_SLOT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let new_sp = STACK_BASE_FIRST + slot * STACK_BASE_STRIDE;
    match patch_wasm_sp_init(wasm, new_sp) {
        Ok(old_sp) => {
            eprintln!(
                "[azul-web] M9-3: relocated stack for {} (slot {}): SP {} → {}",
                output_stem, slot, old_sp, new_sp,
            );
        }
        Err(e) => {
            eprintln!(
                "[azul-web] M9-3: failed to patch SP for {} (slot {}): {} — cross-module \
                 calls into this wasm may corrupt mini's stack",
                output_stem, slot, e,
            );
        }
    }
}

/// M9-3b: inject user-binary data sections as new wasm Data segments
/// in mini.wasm. Called once per mini-wasm link. The segments mirror
/// the user binary's __cstring / __const / __data into wasm memory
/// at the low-32-bit-truncated offsets — what lifted user-binary code
/// expects to find via `adrp + ldr/add`.
///
/// Best-effort: any wasm-format mismatch logs and returns silently;
/// the wasm still loads but lifted user code reading const strings
/// will see zero bytes (the wasm linear memory's default).
#[cfg(feature = "web-transpiler")]
fn inject_user_binary_data_segments(
    wasm: &mut Vec<u8>,
    accessed_pages: &std::collections::HashSet<usize>,
    accessed_ranges: &std::collections::HashSet<(usize, usize)>,
    output_stem: &str,
) {
    // M9-review + per-page (post-review): the synth-addr lift means
    // every native page has a predictable wasm offset. Combined with
    // the set of pages we KNOW the lifted code reads (computed by
    // scanning each function's bytes for `adrp`), we only need to
    // mirror those specific 4 KiB pages — not entire `__const`
    // sections.
    //
    // Without the per-page filter we'd ship ~27 MiB of libazul data
    // per wasm (mostly LLVM/LLD/remill string tables that the user's
    // cb never touches). With it: a few hundred KiB for typical
    // callbacks.
    let table = match super::symbol_table::get() {
        Some(t) => t,
        None => return,
    };
    let segments = collect_synth_data_pages(table, accessed_pages, accessed_ranges);
    if segments.is_empty() {
        eprintln!(
            "[azul-web] M9-after-review: 0 data pages to mirror in {} \
             (accessed_pages set is empty or no pages fall in tracked images)",
            output_stem,
        );
        return;
    }
    let total_bytes: usize = segments.iter().map(|(_, b)| b.len()).sum();
    let pre_len = wasm.len();
    match patch_wasm_add_data_segments(wasm, &segments) {
        Ok(added) => {
            eprintln!(
                "[azul-web] M9-after-review: {} mirrored {} data pages \
                 ({} bytes total, {:.2} MiB) → wasm {} → {} bytes",
                output_stem, added, total_bytes,
                total_bytes as f64 / 1024.0 / 1024.0,
                pre_len, wasm.len(),
            );
        }
        Err(e) => {
            eprintln!(
                "[azul-web] M9-after-review: failed to inject data segments \
                 into {} ({} bytes): {} — lifted const reads will see zero bytes",
                output_stem, total_bytes, e,
            );
        }
    }
}

/// M9-after-review: collect mirrored data for ONLY the specific
/// 4 KiB pages the lifted code reads. `accessed_pages` is the set
/// of NATIVE page addresses harvested by [`scan_arm64_adrp_pages`]
/// across every function whose lift contributed to the wasm being
/// linked. For each page in the set that falls inside a tracked
/// image's text+data range, mirror the page's 4 KiB at the
/// corresponding SYNTH offset.
///
/// Replaces the previous "mirror entire sections" approach that
/// caused mini.wasm to balloon to 27 MiB even for hello-world.
/// Per-page is bounded by what the cb actually touches — typically
/// a few dozen pages = a few hundred KiB.
#[cfg(feature = "web-transpiler")]
fn collect_synth_data_pages(
    table: &super::symbol_table::SymbolTable,
    accessed_pages: &std::collections::HashSet<usize>,
    accessed_ranges: &std::collections::HashSet<(usize, usize)>,
) -> Vec<(u32, Vec<u8>)> {
    const PAGE_SIZE: usize = 4096;
    let mut out: Vec<(u32, Vec<u8>)> = Vec::new();
    let mut translated_total = 0usize;
    let mut skipped: usize = 0;
    // M10-E1: for each native page, collect every precise range that
    // falls inside it. When non-empty, that page mirrors ONLY those
    // ranges (saves 90%+ of the 4 KiB whole-page bytes). When empty,
    // fall back to whole-page mirror (covers patterns the precise
    // scanner doesn't recognize).
    let mut ranges_by_page: std::collections::HashMap<usize, Vec<(usize, usize)>> =
        std::collections::HashMap::new();
    for (addr, len) in accessed_ranges {
        let page = addr & !0xFFF;
        ranges_by_page.entry(page).or_default().push((*addr, *len));
        // Range may straddle a page boundary — credit the next page too.
        let end = addr.wrapping_add(*len).saturating_sub(1);
        let end_page = end & !0xFFF;
        if end_page != page {
            ranges_by_page.entry(end_page).or_default().push((*addr, *len));
        }
    }
    let mut precise_pages = 0usize;
    let mut precise_bytes_kept = 0usize;
    let mut fallback_pages = 0usize;
    // M12.5d diagnostic: when set, ignore precise ranges and mirror
    // every accessed page in full (4 KiB, zero-trimmed). Tests the
    // hypothesis that the cascade reads const-pool data via patterns
    // `scan_arm64_adrp_accesses` doesn't recognize — those addresses
    // read back zero under precise-only mirroring (0 whole-page
    // fallbacks observed in the cascade build).
    let force_whole_page = std::env::var_os("AZ_FORCE_WHOLE_PAGE").is_some();
    // M12.7: pages reached only INDIRECTLY via a mirrored pointer (never a
    // direct `adrp`) — e.g. hashbrown's static EMPTY_GROUP, pointed at by the
    // empty-table singleton's rebased `ctrl`. Such a page is not in
    // accessed_pages, so unmirrored it reads ZERO; an all-zero control group
    // looks ALL-FULL (EMPTY=0xFF) → RawIterRange loops forever. Collect every
    // rebased pointer's target page below and mirror them to fixpoint.
    let mut visited: std::collections::HashSet<usize> =
        accessed_pages.iter().copied().collect();
    let mut pending_targets: Vec<usize> = Vec::new();
    for native_page in accessed_pages {
        let native_page = *native_page;
        // Find which image this page belongs to.
        let Some(synth_page) = table.native_to_synth(native_page) else {
            skipped += 1;
            if std::env::var_os("AZ_WASM_MIRROR_TRACE").is_some() {
                eprintln!(
                    "[azul-web] mirror SKIP native_page=0x{:x} — not in any tracked image",
                    native_page,
                );
            }
            continue;
        };
        // M10-E1: if we have precise ranges for this page, mirror
        // just those byte windows instead of the whole 4 KiB. The
        // ranges' bytes get pointer-translated below.
        if let Some(ranges) = ranges_by_page.get(&native_page).filter(|_| !force_whole_page) {
            // Build a bitmap of which bytes are needed within the page
            // (handles overlapping / adjacent ranges naturally).
            let mut needed = [false; PAGE_SIZE];
            for (addr, len) in ranges {
                let in_page_start = addr.saturating_sub(native_page).min(PAGE_SIZE);
                let in_page_end = (addr + len)
                    .saturating_sub(native_page)
                    .min(PAGE_SIZE);
                for b in in_page_start..in_page_end {
                    needed[b] = true;
                }
            }
            // Read the page (we'll subset below).
            let raw_page = unsafe {
                core::slice::from_raw_parts(native_page as *const u8, PAGE_SIZE)
            };
            // Collect run starts/ends from the bitmap with a merge
            // tolerance: ≤16 byte gaps between needed bytes stay in
            // the same segment (per-segment header is ~5 bytes).
            const MERGE_GAP: usize = 16;
            let mut i = 0;
            let mut total_in_page = 0usize;
            while i < PAGE_SIZE {
                if !needed[i] {
                    i += 1;
                    continue;
                }
                let start = i;
                let mut end = i + 1;
                while end < PAGE_SIZE {
                    if needed[end] {
                        end += 1;
                        continue;
                    }
                    // Peek-ahead: if there's a needed byte within MERGE_GAP, keep going.
                    let mut peek = end;
                    let mut peek_found = false;
                    while peek < PAGE_SIZE && peek - end < MERGE_GAP {
                        if needed[peek] {
                            peek_found = true;
                            break;
                        }
                        peek += 1;
                    }
                    if peek_found {
                        end = peek + 1;
                    } else {
                        break;
                    }
                }
                // Capture this run's bytes + apply pointer translation.
                let mut run = raw_page[start..end].to_vec();
                let mut translated_in_run = 0usize;
                let run_off_in_page = start;
                for chunk_start in (0..run.len()).step_by(8) {
                    if chunk_start + 8 > run.len() {
                        break;
                    }
                    // Only translate at 8-byte aligned offsets within
                    // the page (matches the original page-mirror logic).
                    if (run_off_in_page + chunk_start) % 8 != 0 {
                        continue;
                    }
                    let value = u64::from_le_bytes([
                        run[chunk_start],
                        run[chunk_start + 1],
                        run[chunk_start + 2],
                        run[chunk_start + 3],
                        run[chunk_start + 4],
                        run[chunk_start + 5],
                        run[chunk_start + 6],
                        run[chunk_start + 7],
                    ]);
                    if let Some(synth) = table.native_to_synth(value as usize) {
                        let synth_bytes = (synth as u64).to_le_bytes();
                        run[chunk_start..chunk_start + 8].copy_from_slice(&synth_bytes);
                        translated_in_run += 1;
                        pending_targets.push(value as usize & !0xFFF);
                    }
                }
                translated_total += translated_in_run;
                total_in_page += run.len();
                out.push(((synth_page + start) as u32, run));
                i = end;
            }
            precise_pages += 1;
            precise_bytes_kept += total_in_page;
            continue;
        }
        // No precise ranges for this page — fall back to whole-page
        // mirror (the legacy M9-after-review path).
        fallback_pages += 1;
        if std::env::var_os("AZ_WASM_MIRROR_TRACE").is_some() {
            eprintln!(
                "[azul-web] mirror native_page=0x{:x} → synth=0x{:x} (whole-page fallback)",
                native_page, synth_page,
            );
        }
        // SAFETY: pages reachable from `adrp` targets in lifted code
        // are in mapped image segments — the loader put them there
        // and they stay mapped for the process lifetime. Reading
        // 4 KiB is safe; reads past the segment end are zero-filled
        // by the loader.
        let mut bytes = unsafe {
            core::slice::from_raw_parts(native_page as *const u8, PAGE_SIZE).to_vec()
        };
        // M9-after-review v3: pointer translation in mirrored data.
        // Sections like `__DATA_CONST.__got` contain native runtime
        // addresses (function pointers to libsystem stubs, type-id
        // pointer values, etc.). When lifted code loads one of these
        // and derefs it, the address truncates to a wasm offset past
        // the 128 MiB initial memory → OOB trap. Translate every
        // 8-byte aligned value that falls in a tracked image's
        // runtime range to the corresponding synth address.
        let mut translated_in_page = 0usize;
        for chunk_start in (0..PAGE_SIZE).step_by(8) {
            if chunk_start + 8 > PAGE_SIZE {
                break;
            }
            let value = u64::from_le_bytes([
                bytes[chunk_start],
                bytes[chunk_start + 1],
                bytes[chunk_start + 2],
                bytes[chunk_start + 3],
                bytes[chunk_start + 4],
                bytes[chunk_start + 5],
                bytes[chunk_start + 6],
                bytes[chunk_start + 7],
            ]);
            if let Some(synth) = table.native_to_synth(value as usize) {
                let synth_bytes = (synth as u64).to_le_bytes();
                bytes[chunk_start..chunk_start + 8].copy_from_slice(&synth_bytes);
                translated_in_page += 1;
                pending_targets.push(value as usize & !0xFFF);
            }
        }
        translated_total += translated_in_page;
        out.push((synth_page as u32, bytes));
    }
    // M12.7: transitively mirror the pages that mirrored pointers point INTO
    // (see note above). Whole-page mirror each, rebasing its own pointers, and
    // queue THEIR targets — to fixpoint, bounded so a pathological pointer
    // graph can't explode the data section.
    let mut transitive_pages = 0usize;
    let mut budget = 20_000usize;
    while let Some(tp) = pending_targets.pop() {
        if budget == 0 {
            break;
        }
        if !visited.insert(tp) {
            continue;
        }
        let Some(synth_tp) = table.native_to_synth(tp) else {
            continue;
        };
        budget -= 1;
        // SAFETY: `tp` is in a tracked image's mapped range (native_to_synth
        // returned Some) → reading its 4 KiB page is safe.
        let mut bytes =
            unsafe { core::slice::from_raw_parts(tp as *const u8, PAGE_SIZE).to_vec() };
        for cs in (0..PAGE_SIZE).step_by(8) {
            let v = u64::from_le_bytes(bytes[cs..cs + 8].try_into().unwrap());
            if let Some(synth) = table.native_to_synth(v as usize) {
                bytes[cs..cs + 8].copy_from_slice(&(synth as u64).to_le_bytes());
                pending_targets.push(v as usize & !0xFFF);
            }
        }
        out.push((synth_tp as u32, bytes));
        transitive_pages += 1;
    }
    if transitive_pages > 0 {
        eprintln!(
            "[azul-web] M12.7: transitively mirrored {} pointer-target pages",
            transitive_pages,
        );
    }
    out.sort_by_key(|(off, _)| *off);
    if translated_total > 0 {
        eprintln!(
            "[azul-web] M9-after-review v3: pointer-translated {} native→synth values in mirrored pages",
            translated_total,
        );
    }
    if precise_pages > 0 || fallback_pages > 0 {
        eprintln!(
            "[azul-web] M10-E1 mirror: {} precise pages ({} bytes kept), {} whole-page fallbacks",
            precise_pages, precise_bytes_kept, fallback_pages,
        );
    }
    // M10-E1: zero-trim every page into one-or-more non-zero runs.
    // wasm linear memory defaults to zero, so any all-zero range can
    // be elided from the data section. Each non-zero run becomes its
    // own data segment at `synth_page + run_start_offset` with
    // length `run_len`. Saves 30–80% of data-section bytes for
    // sparse pages (typical for __DATA_CONST holding a few pointers
    // surrounded by alignment padding).
    //
    // Run merger: gaps shorter than `MIN_GAP` collapse into the
    // surrounding run. Each split adds ~5 bytes of segment header
    // overhead (LEB128 offset + size + memidx); a 16-byte gap that
    // would otherwise become two segments is cheaper to ship as
    // one segment with the gap included.
    const MIN_GAP: usize = 16;
    let pre_trim_bytes: usize = out.iter().map(|(_, b)| b.len()).sum();
    let trimmed: Vec<(u32, Vec<u8>)> = out
        .into_iter()
        .flat_map(|(base, bytes)| split_nonzero_runs(base, bytes, MIN_GAP))
        .collect();
    let post_trim_bytes: usize = trimmed.iter().map(|(_, b)| b.len()).sum();
    let saved = pre_trim_bytes.saturating_sub(post_trim_bytes);
    if saved > 0 {
        eprintln!(
            "[azul-web] M10-E1 zero-trim: data bytes {} → {} ({} segments, saved {} bytes)",
            pre_trim_bytes, post_trim_bytes, trimmed.len(), saved,
        );
    }
    let out = trimmed;
    if skipped > 0 && std::env::var_os("AZ_WASM_MIRROR_TRACE").is_some() {
        eprintln!(
            "[azul-web] mirror: skipped {} accessed pages (not in any tracked image)",
            skipped,
        );
    }
    out
}

/// M10-E1 — split a mirrored page into one-or-more non-zero runs,
/// each emitted as a separate data segment at `base + run_offset`.
///
/// A "run" is a maximal sequence of bytes that does NOT contain a
/// zero-gap longer than `min_gap`. Runs shorter than `min_gap` of
/// zeros stay merged into the surrounding run to avoid paying
/// per-segment header overhead (~5 bytes per data segment for the
/// LEB128-encoded offset + size + memidx).
///
/// Leading and trailing zeros are always trimmed (no overhead since
/// they're entirely outside any run).
fn split_nonzero_runs(
    base: u32,
    bytes: Vec<u8>,
    min_gap: usize,
) -> Vec<(u32, Vec<u8>)> {
    let mut runs: Vec<(u32, Vec<u8>)> = Vec::new();
    let n = bytes.len();
    let mut i = 0usize;
    while i < n {
        // Skip leading zeros.
        while i < n && bytes[i] == 0 {
            i += 1;
        }
        if i >= n {
            break;
        }
        // Start of a non-zero run. Extend until we hit a zero-gap
        // ≥ min_gap or end of buffer.
        let run_start = i;
        let mut run_end = i;
        while run_end < n {
            // Advance run_end through nonzero bytes.
            while run_end < n && bytes[run_end] != 0 {
                run_end += 1;
            }
            // Peek the gap: count consecutive zeros from run_end.
            let mut gap = run_end;
            while gap < n && bytes[gap] == 0 {
                gap += 1;
            }
            let gap_len = gap - run_end;
            if gap_len >= min_gap || gap == n {
                // Long enough gap (or end of buffer): close this run.
                break;
            }
            // Merge the gap into the current run.
            run_end = gap;
        }
        let run_offset = run_start as u32;
        let run_bytes = bytes[run_start..run_end].to_vec();
        runs.push((base + run_offset, run_bytes));
        i = run_end;
    }
    // If a page collapsed to nothing (entirely zero), preserve a
    // 1-byte zero segment so downstream debugging can still see the
    // page was accessed — but only when explicitly requested. By
    // default, empty pages emit no segments.
    runs
}

/// M9-review (UNUSED now — kept temporarily for git history;
/// remove after the per-page mirror lands stable). Walked per-image
/// rebases + mirrored entire data sections, producing mini.wasm
/// payloads of ~27 MiB for hello-world due to libazul's LLVM/LLD/
/// remill string tables.
#[allow(dead_code)]
#[cfg(feature = "web-transpiler")]
fn collect_synth_data_segments_legacy(
    table: &super::symbol_table::SymbolTable,
) -> Vec<(u32, Vec<u8>)> {
    let mut out: Vec<(u32, Vec<u8>)> = Vec::new();
    // Per-section size cap (skip pathologically large sections;
    // mini.wasm bloat scales linearly with mirrored bytes).
    const PER_SECTION_LIMIT: usize = 32 * 1024 * 1024;
    // Synth-offset upper bound: just below the bump-heap base in
    // `emit_helper_ir`'s @__az_bump_ptr init (96 MiB). Sections
    // landing past this would overlap with the heap.
    const SYNTH_OFFSET_LIMIT: u64 = 96 * 1024 * 1024;
    for rebase in table.image_rebases() {
        // Re-derive each image's data sections by re-parsing its
        // bytes. The on-disk Vec<u8> stays alive in SymbolTable's
        // `image_bytes` for this purpose.
        let path = std::path::Path::new(&rebase.path);
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let parsed = match goblin::Object::parse(&bytes) {
            Ok(p) => p,
            Err(_) => continue,
        };
        // Section enumeration delegated to the helper that already
        // knows the per-format quirks; just IGNORE its truncated-
        // offset filter (we use our own synth-aware filter below).
        let sections: Vec<(u64, u64)> = match parsed {
            goblin::Object::Mach(goblin::mach::Mach::Binary(macho)) => {
                super::symbol_table::collect_macho_low32_sections(
                    &macho, &bytes, /*slide=*/ 0, u32::MAX,
                )
            }
            goblin::Object::Mach(goblin::mach::Mach::Fat(fat)) => {
                match super::symbol_table::pick_fat_slice(&fat, &bytes) {
                    Ok(Some(macho)) => super::symbol_table::collect_macho_low32_sections(
                        &macho, &bytes, 0, u32::MAX,
                    ),
                    _ => Vec::new(),
                }
            }
            goblin::Object::Elf(elf) => super::symbol_table::collect_elf_low32_sections(
                &elf, &bytes, 0, u32::MAX,
            ),
            _ => Vec::new(),
        };
        // Map file_vmaddr → synth offset.
        // image_native_text_base = rebase.native_base (the lowest non-PAGEZERO
        // segment's vmaddr+slide). For Mach-O executables that's typically
        // 0x100000000+slide; for dylibs typically slide.
        // The image's file_vmaddr is `section_addr` (as collect_* returns).
        // file_offset_within_image = section_addr - (native_base - slide)
        // synth_offset = rebase.synth_base + file_offset_within_image
        //
        // We don't have the slide here, but rebase.native_base = file_native_min + slide.
        // file_native_min was computed in `assign_synthetic_addresses` from the
        // image's non-PAGEZERO segments.
        for (section_file_vmaddr, section_size) in sections {
            if section_size == 0 || section_size as usize > PER_SECTION_LIMIT {
                continue;
            }
            // section's live address = file_vmaddr + slide
            //                       = file_vmaddr + (native_base - file_native_min)
            // But we don't store file_native_min. Workaround: assume the
            // rebase's native_base IS the runtime address corresponding to
            // file_vmaddr 0 (for dylibs) or the first non-PAGEZERO segment
            // (for execs). For Mach-O execs the first non-PAGEZERO segment
            // starts at file_vmaddr 0x100000000; for dylibs it's 0.
            // Determine by checking if any rebase has a section starting
            // close to 0 (dylib) vs 0x100000000 (exec).
            //
            // Pragmatic fix: just use file_vmaddr directly modulo the image's
            // address-of-text. Since `assign_synthetic_addresses` set
            // rebase.synth_base such that `synth_addr = synth_base + (canonical_addr - native_base)`,
            // and `canonical_addr = file_vmaddr + slide`, we have
            // `synth_offset = synth_base + (file_vmaddr + slide - native_base)`.
            // `slide - native_base = -file_native_min`, so
            // `synth_offset = synth_base + (file_vmaddr - file_native_min)`.
            //
            // We don't directly track file_native_min on the rebase, but
            // it's `image_native_base - slide`. Without the slide, fall
            // back to using `section_file_vmaddr` modulo a heuristic
            // image-base detection: for Mach-O execs, mask away the
            // 0x100000000 PAGEZERO offset.
            let file_offset_within_image = if section_file_vmaddr >= 0x1_0000_0000 {
                // Mach-O exec: __TEXT starts at 0x100000000
                section_file_vmaddr - 0x1_0000_0000
            } else {
                section_file_vmaddr
            };
            let synth_offset = (rebase.synth_base as u64)
                .wrapping_add(file_offset_within_image);
            if synth_offset == 0 || synth_offset + section_size > SYNTH_OFFSET_LIMIT {
                continue;
            }
            // The bytes live at `native_base + file_offset_within_image`
            // (= file_vmaddr + slide once slide-correction is folded in).
            // Equivalently: rebase.native_base + file_offset_within_image.
            let live_addr = rebase.native_base.wrapping_add(file_offset_within_image as usize);
            let mirrored = unsafe {
                core::slice::from_raw_parts(live_addr as *const u8, section_size as usize).to_vec()
            };
            out.push((synth_offset as u32, mirrored));
        }
    }
    out.sort_by_key(|(off, _)| *off);
    out.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    out
}

/// Append the given `(wasm_offset, bytes)` pairs as new active Data
/// segments to `wasm`'s Data section. Creates a Data section if one
/// doesn't exist. Also updates the `DataCount` section (id 12) when
/// present (used by reference-typed wasm modules).
fn patch_wasm_add_data_segments(
    wasm: &mut Vec<u8>,
    segments: &[(u32, Vec<u8>)],
) -> Result<usize, String> {
    if segments.is_empty() {
        return Ok(0);
    }
    if wasm.len() < 8 || &wasm[..8] != b"\x00asm\x01\x00\x00\x00" {
        return Err("not a wasm module".into());
    }

    // Encode each new segment: [mem_kind=0x00, offset_expr, data_size, data_bytes]
    // offset_expr = i32.const N end = 0x41 <sleb128> 0x0B.
    fn encode_segment(offset: u32, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + data.len());
        out.push(0x00); // mem_kind: active, memidx=0 implicit
        out.push(0x41); // i32.const
        out.extend(encode_sleb128(offset as i64));
        out.push(0x0B); // end
        out.extend(encode_uleb128(data.len() as u64));
        out.extend_from_slice(data);
        out
    }
    let new_segment_blobs: Vec<Vec<u8>> = segments
        .iter()
        .map(|(off, data)| encode_segment(*off, data))
        .collect();
    let added_count = new_segment_blobs.len();
    let new_segments_concat: Vec<u8> = new_segment_blobs.into_iter().flatten().collect();

    // Scan for Data section (id 11). If found, append; if not, create.
    let mut i = 8;
    let mut data_section: Option<(usize, usize, usize)> = None; // (size_offset, payload_start, payload_end)
    let mut data_count_section_size_offset: Option<usize> = None;
    let mut insert_pos: usize = wasm.len();  // for new Data section if none exists

    while i < wasm.len() {
        let section_id = wasm[i];
        i += 1;
        let (section_size, leb_bytes) = decode_uleb128(&wasm[i..])
            .ok_or_else(|| "bad section-size uleb128".to_string())?;
        let size_offset = i;
        i += leb_bytes;
        let payload_start = i;
        let payload_end = i + section_size as usize;
        if payload_end > wasm.len() {
            return Err(format!("section {} overruns wasm", section_id));
        }
        if section_id == 11 {
            data_section = Some((size_offset, payload_start, payload_end));
        }
        if section_id == 12 {
            data_count_section_size_offset = Some(size_offset);
        }
        // Custom sections (id 0) and the Data section can be at the end;
        // anywhere after Function-Section (id 10) is fine to insert a
        // new Data section. We pick after section 10 (Code) if Data
        // doesn't exist yet.
        if section_id == 10 {
            insert_pos = payload_end;
        }
        i = payload_end;
    }

    if let Some((size_offset, payload_start, payload_end)) = data_section {
        // Modify existing Data section: increment count, append segments.
        let (count, count_lb) = decode_uleb128(&wasm[payload_start..payload_end])
            .ok_or_else(|| "bad Data count uleb128".to_string())?;
        let new_count = count + added_count as u64;
        let new_count_bytes = encode_uleb128(new_count);

        let old_count_len = count_lb;
        let count_delta = new_count_bytes.len() as i64 - old_count_len as i64;

        // Update section size: old + (count_delta + new_segments_size)
        let new_section_size =
            (payload_end - payload_start) as i64 + count_delta + new_segments_concat.len() as i64;
        if new_section_size < 0 {
            return Err("negative section size".into());
        }
        let new_size_bytes = encode_uleb128(new_section_size as u64);

        // Splice in the new bytes. Order matters since we're modifying
        // ranges in the original buffer.
        // 1. Replace count uleb128.
        wasm.splice(payload_start..payload_start + count_lb, new_count_bytes);
        // The new payload_end shifts by count_delta. The append position
        // is at the (shifted) old payload_end.
        let new_payload_end_pos = (payload_end as i64 + count_delta) as usize;
        wasm.splice(new_payload_end_pos..new_payload_end_pos, new_segments_concat);
        // 2. Update section size uleb128 at size_offset (which is BEFORE
        // any changes we just made, so it's still valid).
        let old_size_len = leb_count_at(wasm, size_offset);
        wasm.splice(size_offset..size_offset + old_size_len, new_size_bytes);
    } else {
        // Create a new Data section.
        let mut payload: Vec<u8> = Vec::new();
        payload.extend(encode_uleb128(added_count as u64));
        payload.extend(new_segments_concat);

        let mut new_section: Vec<u8> = Vec::new();
        new_section.push(11);
        new_section.extend(encode_uleb128(payload.len() as u64));
        new_section.extend(payload);

        wasm.splice(insert_pos..insert_pos, new_section);
    }

    // Update DataCount section (id 12) if present — its single uleb
    // payload must equal the number of data segments.
    if let Some(size_offset) = data_count_section_size_offset {
        // Re-locate after our modifications: rescan for section 12.
        let mut i = 8;
        let mut found_dc: Option<(usize, usize, usize)> = None;
        while i < wasm.len() {
            let section_id = wasm[i];
            i += 1;
            let (section_size, leb_bytes) = decode_uleb128(&wasm[i..])
                .ok_or_else(|| "rescan: bad section-size uleb128".to_string())?;
            let new_size_offset = i;
            i += leb_bytes;
            let payload_start = i;
            let payload_end = i + section_size as usize;
            if section_id == 12 {
                found_dc = Some((new_size_offset, payload_start, payload_end));
                break;
            }
            i = payload_end;
        }
        let _ = size_offset;
        if let Some((dc_size_offset, payload_start, payload_end)) = found_dc {
            let (old_count, _) = decode_uleb128(&wasm[payload_start..payload_end])
                .ok_or_else(|| "bad DataCount uleb128".to_string())?;
            let new_count = old_count + added_count as u64;
            let new_count_bytes = encode_uleb128(new_count);
            let new_payload_len = new_count_bytes.len();
            let new_section_size_bytes = encode_uleb128(new_payload_len as u64);
            let old_size_len = leb_count_at(wasm, dc_size_offset);
            // Replace section payload + size.
            wasm.splice(payload_start..payload_end, new_count_bytes);
            wasm.splice(dc_size_offset..dc_size_offset + old_size_len, new_section_size_bytes);
        }
    }

    Ok(added_count)
}

/// Count the number of bytes in the ULEB128 starting at `wasm[pos]`.
fn leb_count_at(wasm: &[u8], pos: usize) -> usize {
    let mut n = 0;
    for &b in &wasm[pos..] {
        n += 1;
        if (b & 0x80) == 0 {
            break;
        }
    }
    n
}

/// Replace global[0]'s init expression with `i32.const new_sp; end`.
/// Returns the previous SP value on success. Assumes global[0] is the
/// stack pointer (the wasm-ld convention).
fn patch_wasm_sp_init(wasm: &mut Vec<u8>, new_sp: u32) -> Result<i64, String> {
    if wasm.len() < 8 || &wasm[..8] != b"\x00asm\x01\x00\x00\x00" {
        return Err("not a wasm module".into());
    }
    let mut i = 8;
    while i < wasm.len() {
        let section_id = wasm[i];
        i += 1;
        let (section_size, leb_bytes) = decode_uleb128(&wasm[i..])
            .ok_or_else(|| "bad section-size uleb128".to_string())?;
        let size_offset = i;  // byte offset of section size's first byte
        i += leb_bytes;
        let payload_start = i;
        let payload_end = i + section_size as usize;
        if payload_end > wasm.len() {
            return Err(format!("section {} overruns wasm", section_id));
        }
        if section_id != 6 {
            // not Global; skip
            i = payload_end;
            continue;
        }
        // Global section payload: [count uleb, [global_type, init_expr]+]
        let (count, count_lb) = decode_uleb128(&wasm[payload_start..payload_end])
            .ok_or_else(|| "bad global count uleb128".to_string())?;
        if count == 0 {
            return Err("no globals".into());
        }
        let mut p = payload_start + count_lb;
        // global[0] type: 1 byte value_type (i32 = 0x7F), 1 byte mut flag.
        if wasm[p] != 0x7F {
            return Err(format!("global[0] is not i32 (type = 0x{:02x})", wasm[p]));
        }
        p += 2;  // skip value_type + mut
        // init_expr starts with an opcode.
        if wasm[p] != 0x41 {
            return Err(format!("global[0] init not i32.const (opcode = 0x{:02x})", wasm[p]));
        }
        let init_start = p;
        p += 1;
        // Decode the existing sleb128 value (for logging) and skip to end-marker.
        let (old_value, val_bytes) = decode_sleb128(&wasm[p..])
            .ok_or_else(|| "bad init sleb128".to_string())?;
        p += val_bytes;
        if wasm[p] != 0x0B {
            return Err(format!("expected init end marker (0x0B), got 0x{:02x}", wasm[p]));
        }
        let init_end = p + 1;  // exclusive

        // Build the replacement bytes: i32.const NEW_SP, end.
        let mut new_init: Vec<u8> = vec![0x41];
        new_init.extend(encode_sleb128(new_sp as i64));
        new_init.push(0x0B);

        let old_init_len = init_end - init_start;
        let new_init_len = new_init.len();
        let size_delta = new_init_len as i64 - old_init_len as i64;

        // Splice the init expression.
        wasm.splice(init_start..init_end, new_init);

        // Update the section size uleb128. New section size = old + delta.
        let new_section_size = (section_size as i64) + size_delta;
        if new_section_size < 0 {
            return Err("negative section size after patch".into());
        }
        let new_size_bytes = encode_uleb128(new_section_size as u64);
        wasm.splice(size_offset..size_offset + leb_bytes, new_size_bytes);

        return Ok(old_value);
    }
    Err("no Global section found".into())
}

/// Decode an unsigned LEB128 from the start of `bytes`. Returns
/// `(value, n_bytes_consumed)` or `None` on overflow / truncation.
fn decode_uleb128(bytes: &[u8]) -> Option<(u64, usize)> {
    let mut value: u64 = 0;
    let mut shift: u32 = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if shift >= 64 {
            return None;
        }
        value |= ((b & 0x7F) as u64) << shift;
        if (b & 0x80) == 0 {
            return Some((value, i + 1));
        }
        shift += 7;
    }
    None
}

/// Decode a signed LEB128 from the start of `bytes`. Returns
/// `(value, n_bytes_consumed)` or `None` on overflow / truncation.
fn decode_sleb128(bytes: &[u8]) -> Option<(i64, usize)> {
    let mut value: i64 = 0;
    let mut shift: u32 = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if shift >= 64 {
            return None;
        }
        value |= ((b & 0x7F) as i64) << shift;
        shift += 7;
        if (b & 0x80) == 0 {
            // Sign-extend if the value bit at position `shift-1` is set.
            if shift < 64 && (b & 0x40) != 0 {
                value |= -1i64 << shift;
            }
            return Some((value, i + 1));
        }
    }
    None
}

/// Encode `value` as unsigned LEB128.
fn encode_uleb128(mut value: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(5);
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            return out;
        }
    }
}

/// Encode `value` as signed LEB128.
fn encode_sleb128(mut value: i64) -> Vec<u8> {
    let mut out = Vec::with_capacity(5);
    loop {
        let byte = (value & 0x7F) as u8;
        let sign_bit_set = (byte & 0x40) != 0;
        // Arithmetic right-shift by 7.
        value >>= 7;
        let done = (value == 0 && !sign_bit_set) || (value == -1 && sign_bit_set);
        let with_continuation = if done { byte } else { byte | 0x80 };
        out.push(with_continuation);
        if done {
            return out;
        }
    }
}

/// Memory-import vs own-memory selection for
/// [`RemillTranspiler::link_objects_to_wasm`]. Per-cb / per-layout
/// wasms import `env.memory` so they share linear address space
/// with the mini wasm; the mini wasm itself ships an exported
/// `memory` that the JS bootstrap routes to all other wasms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryMode {
    /// `--initial-memory=N` — wasm-ld declares + exports its own
    /// `memory`. Used by the mini wasm (the source of truth for
    /// shared memory).
    OwnMemory,
    /// `--import-memory` — wasm-ld emits an import for `env.memory`
    /// instead of declaring its own. JS supplies mini's exported
    /// memory at instantiate time.
    ImportMemory,
}

/// M10-D — output of [`RemillTranspiler::lift_boundary_to_wasm`]. One
/// per `api.json::Framework` symbol referenced by any cb / layout /
/// mini-shard lift in the running server. The shard exports the raw
/// `sub_<canonical_hex>` body so other wasms can import and call it
/// directly via `env.sub_<canonical_hex>`.
#[derive(Debug, Clone)]
pub struct BoundaryShard {
    /// Canonical (post-PLT-chase) address of the boundary symbol
    /// in the loaded image. Used as the dedup key when the
    /// orchestrator unions used-boundary sets across many lifts.
    pub canonical_addr: usize,
    /// Canonical C-API name (`AzRefAny_clone`, `AzDom_addChild`, …).
    pub canonical_name: String,
    /// The symbol name the shard's wasm EXPORTS — always
    /// `sub_<canonical_synth_hex>`. JS wiring sets
    /// `env.sub_<canonical_synth_hex> = boundaryShard.exports[body_export]`
    /// when instantiating dependent wasms.
    pub body_export: String,
    /// Internal wrapper export name (`AzBoundary_<synth_hex>`) — kept
    /// for diagnostics; never called from JS or other wasms.
    pub wrapper_export: String,
    /// FNV-1a 64-bit content hash of the wasm bytes. Used in the
    /// served URL `/az/fn/<canonical_name>.<content_hash>.wasm` for
    /// browser cache busting.
    pub content_hash: String,
    /// The shard's wasm bytes (the contents of the served wasm file).
    pub wasm_bytes: Vec<u8>,
    /// Canonical addresses of every OTHER boundary the shard's BFS
    /// surfaced as a transitive dep. The orchestrator follows the
    /// chain to ensure every downstream shard gets lifted too.
    pub transitive_boundaries: Vec<usize>,
}

/// Neutralize **recursive** `bl` instructions inside the input
/// buffer before passing it to remill's TraceLifter.
///
/// remill's TraceLifter eagerly follows every `bl` target. When
/// the target lies inside the buffer (a tail-recursive Rust fn,
/// e.g. `Dom::fixup_children_estimated`), it adds the same
/// trace_addr to the work list, then re-enters the outer loop —
/// but the per-trace state isn't reset between iterations. The
/// observable result is unbounded module growth (15+ GiB peak,
/// SIGKILL'd by macOS memorystatus on 32 GiB hosts).
///
/// Our pipeline lifts ONE function per remill invocation, so the
/// "follow bl into a separate trace" behavior is undesirable
/// anyway. We rewrite a recursive `bl <target-inside-buffer>` to
/// a `bl <huge-positive-offset>` — same Cond=DirectFunctionCall
/// category, but the target lies outside the readable buffer, so
/// remill emits `__remill_function_call(missing_block)` and
/// continues. At wasm-link time the boundary stub resolves the
/// recursion via the imported function name.
///
/// Encoding:
///   bl: `100101 imm26`  -> top 6 bits 0x94/0x97 depending on sign
///   imm26 is sign-extended <<2 to get the byte offset.
///   For a 116-byte fn, all valid recursive targets fit in
///   [-29, +28] words ≈ ±116 bytes. Anything else escapes.
///
/// Rewrite strategy: any `bl <imm26>` where the resulting target
/// (current_pc + imm26<<2) is within [0, buffer.len()) gets
/// rewritten so the offset is biased by 0x1000000 words (16 MiB).
/// That puts the target far outside the buffer, triggering the
/// "missing bytes" path in remill cleanly.
#[cfg(feature = "web-transpiler")]
fn rewrite_recursive_bl(bytes: &mut [u8]) {
    if bytes.len() < 4 { return; }
    let buf_len = bytes.len() as i64;
    let chunk_count = bytes.len() / 4;
    for i in 0..chunk_count {
        let off = i * 4;
        let insn = u32::from_le_bytes([
            bytes[off], bytes[off+1], bytes[off+2], bytes[off+3],
        ]);
        // bl: bits 31:26 = 100101 (0x25). High byte starts with
        // 0x94 (imm26 sign bit = 0, positive) or 0x97 (sign bit = 1, negative).
        // Mask 0xFC000000 isolates the opcode.
        if (insn & 0xFC00_0000) != 0x9400_0000 {
            continue;
        }
        // Sign-extend imm26 (bits 25:0) to i32.
        let mut imm26 = (insn & 0x03FF_FFFF) as i32;
        if imm26 & 0x0200_0000 != 0 {
            imm26 |= 0xFC00_0000_u32 as i32; // sign-extend
        }
        let byte_offset = (imm26 as i64) << 2;
        let target = (off as i64) + byte_offset;
        // Only rewrite if target lands inside the buffer.
        if target < 0 || target >= buf_len {
            continue;
        }
        // Rewrite to a bl with imm26 = +0x1000000 (16 MiB forward).
        // New offset = 0x4000000 bytes (within imm26 range since
        // imm26 max = 2^25-1 = 0x1FFFFFF and we use 0x1000000).
        let new_insn = 0x9400_0000 | 0x0100_0000_u32;
        let le = new_insn.to_le_bytes();
        bytes[off]   = le[0];
        bytes[off+1] = le[1];
        bytes[off+2] = le[2];
        bytes[off+3] = le[3];
    }
}

/// Rewrite ARMv8.3 LDAPR/LDAPRB/LDAPRH instructions to the
/// pre-ARMv8.3 LDAR/LDARB/LDARH equivalents so remill's decoder
/// (which doesn't know LDAPR) lifts them cleanly. For wasm
/// single-threaded execution the RCpc-vs-acquire relaxation
/// difference doesn't matter — both compile to a plain
/// `__remill_read_memory_*` plus a `__remill_barrier_load_store`.
///
/// LDAPR  (32-bit): `1011 1000 1011 1111 1100 00|Rn|Rt`  0xB8BFC000
/// LDAR   (32-bit): `1000 1000 1101 1111 1111 11|Rn|Rt`  0x88DFFC00
/// LDAPRB (8-bit):  `0011 1000 1011 1111 1100 00|Rn|Rt`  0x38BFC000
/// LDARB  (8-bit):  `0000 1000 1101 1111 1111 11|Rn|Rt`  0x08DFFC00
/// LDAPRH (16-bit): `0111 1000 1011 1111 1100 00|Rn|Rt`  0x78BFC000
/// LDARH  (16-bit): `0100 1000 1101 1111 1111 11|Rn|Rt`  0x48DFFC00
/// LDAPR  (64-bit): `1111 1000 1011 1111 1100 00|Rn|Rt`  0xF8BFC000
/// LDAR   (64-bit): `1100 1000 1101 1111 1111 11|Rn|Rt`  0xC8DFFC00
///
/// Mask 0xFFFFFC00 isolates the upper bits (size, opc, R, A,
/// fixed-1s) and zeros the Rn/Rt fields. So a candidate LDAPR
/// has `bytes & 0xFFFFFC00 == 0xB8BFC000 | (size << 30)` and the
/// rewrite is `bytes |= 0x10001C00 ^ 0x10001C00 = ...`. Simpler
/// to do per width:
#[cfg(feature = "web-transpiler")]
fn rewrite_ldapr_to_ldar(bytes: &mut [u8]) {
    // AArch64 instructions are 4-byte aligned (little-endian on Apple).
    if bytes.len() < 4 { return; }
    let chunk_count = bytes.len() / 4;
    for i in 0..chunk_count {
        let off = i * 4;
        let insn = u32::from_le_bytes([
            bytes[off], bytes[off+1], bytes[off+2], bytes[off+3],
        ]);
        // LDAPR* form: bits 23..21 = 101, bits 15..10 = 110000.
        // Top byte / size: B=0x38, H=0x78, W=0xB8, X=0xF8.
        // LDAR* form:     bits 23..21 = 110, bits 15..10 = 111111.
        // Mask isolates: 0xFFFF_FC00 covers the opcode bits.
        let masked = insn & 0xFFFF_FC00;
        let new = match masked {
            0x38BF_C000 => Some(0x08DF_FC00),  // LDAPRB → LDARB
            0x78BF_C000 => Some(0x48DF_FC00),  // LDAPRH → LDARH
            0xB8BF_C000 => Some(0x88DF_FC00),  // LDAPR_32 → LDAR_32
            0xF8BF_C000 => Some(0xC8DF_FC00),  // LDAPR_64 → LDAR_64
            _ => None,
        };
        if let Some(new_top) = new {
            let rewritten = new_top | (insn & 0x0000_03FF);
            let le = rewritten.to_le_bytes();
            bytes[off]   = le[0];
            bytes[off+1] = le[1];
            bytes[off+2] = le[2];
            bytes[off+3] = le[3];
        }
    }
}

/// Per-call options for [`RemillTranspiler::lift_with_transitive_deps_ex`].
///
/// M11 Sprint 1 added this so the eventloop pipeline can route
/// through the transitive lifter while still bundling hand-written
/// `bump_helpers.o`, owning its own wasm memory, and raising the
/// recursion cap above the per-cb default.
#[derive(Debug, Clone)]
pub struct LiftOpts {
    /// Pre-linked object files to add to the wasm-ld input set
    /// alongside the lifted .o's. Used to bundle hand-written
    /// helpers (e.g. `bump_helpers.o`) into the final wasm.
    pub extra_objects: Vec<PathBuf>,
    /// Extra export names to pass to wasm-ld's `--export=` list.
    /// Used for symbols defined in `extra_objects` that wouldn't
    /// otherwise survive `--gc-sections`.
    pub extra_exports_passthrough: Vec<String>,
    /// Whether the produced wasm owns its memory (mini.wasm) or
    /// imports `env.memory` from another wasm (per-cb / per-layout).
    pub memory_mode: MemoryMode,
    /// Hard cap on the number of fns the BFS can pull into the
    /// lift set. Per-cb lifts default to 256; the eventloop bumps
    /// this to absorb cascade + layout transitive deps.
    pub max_recursive_depth: usize,
    /// Output stem for wasm-ld + diagnostic logs (`<stem>.wasm`).
    pub output_stem: String,
}

impl Default for LiftOpts {
    fn default() -> Self {
        Self {
            extra_objects: Vec::new(),
            extra_exports_passthrough: Vec::new(),
            memory_mode: MemoryMode::ImportMemory,
            max_recursive_depth: 256,
            output_stem: "transitive-lift".to_string(),
        }
    }
}

/// Specifies a root function for [`RemillTranspiler::lift_with_transitive_deps`].
#[derive(Debug, Clone)]
pub struct TransitiveLiftRoot {
    pub fn_name: String,
    pub fn_addr: usize,
    pub fn_size: usize,
    pub sig: CallbackSignature,
    pub export_as: String,
    /// M10-D: additional symbol names to add to wasm-ld's `--export`
    /// list when this root's lift links. The boundary-lift pass uses
    /// this to export `sub_<canonical_hex>` (the raw lifted body)
    /// alongside the wrapper, so cb / layout / mini wasms can import
    /// the boundary's body directly from `env` at instantiate-time.
    /// Default empty for cb / layout / eventloop lifts.
    pub extra_exports: Vec<String>,
}

/// Internal queue item — either a root with a user-chosen wrapper
/// signature/export, or a transitively-reached dependency that gets
/// a no-op default wrapper (its callable surface is the
/// `sub_<addr_hex>` body other lifted code calls by name).
#[derive(Debug)]
enum TransitiveLiftTarget {
    Root(TransitiveLiftRoot),
    Dep {
        name: String,
        addr: usize,
        size: usize,
    },
}

impl Default for RemillTranspiler {
    fn default() -> Self {
        Self::new()
    }
}

impl Transpiler for RemillTranspiler {
    fn lift_function(
        &self,
        fn_name: &str,
        fn_addr: usize,
        fn_size: usize,
        kind: &str,
    ) -> Result<WasmModule, TranspileError> {
        // M8.7c-2: per-callback lift is now a RECURSIVE TRANSITIVE
        // lift, not just one function. Brings in everything the cb
        // reaches (e.g. on_click's MyDataModel_downcastMut →
        // AzRefAny_getDataPtr → AzRefCount_increaseRefmut → ...
        // until known leaves or already-lifted deps). User-facing
        // export is still `callback` (loader.js dispatches by that
        // name); transitive deps get `__az_dep_<addr>` placeholder
        // exports that wasm-ld --gc-sections strips out.
        //
        // M9-1: `kind` selects the wrapper signature. Widget cbs
        // pass `"Callback"`; layout cbs pass `"LayoutCallback"` so
        // the wrapper appends an `out_ptr` arg and seeds the X8
        // hidden-return register for the lifted body to write the
        // returned AzDom through.
        let sig = signature_for_callback_kind(kind);
        let root = TransitiveLiftRoot {
            fn_name: fn_name.to_string(),
            fn_addr,
            fn_size,
            sig,
            export_as: super::WASM_CALLBACK_EXPORT.to_string(),
            extra_exports: Vec::new(),
        };
        self.lift_with_transitive_deps(vec![root])
    }

    fn lift_and_link_framework(
        &self,
        functions: &[(String, usize, usize)],
    ) -> Result<WasmModule, TranspileError> {
        // WB1.2/1.4 minimum-viable: lift each function independently and
        // link the resulting objects into one module. A future revision
        // will batch all IR into a single `llc` invocation to enable
        // cross-function inlining; for now we trade compile-time for a
        // simpler `lift_function` reuse path.
        if functions.is_empty() {
            return Err(TranspileError {
                fn_name: "azul-mini".into(),
                reason: "no framework functions provided".into(),
            });
        }
        let tools = self.tools("azul-mini")?;
        std::fs::create_dir_all(&self.scratch_dir).map_err(|e| TranspileError {
            fn_name: "azul-mini".into(),
            reason: format!("scratch dir: {e}"),
        })?;

        let mut object_paths: Vec<PathBuf> = Vec::with_capacity(functions.len());
        let mut exports: Vec<String> = Vec::with_capacity(functions.len());
        let lift_addr: u64 = 0x100000000;
        let arch_tag = host_arch_tag().ok_or_else(|| TranspileError {
            fn_name: "azul-mini".into(),
            reason: "unsupported host architecture".into(),
        })?;

        for (name, addr, size) in functions {
            let bytes: Vec<u8> = unsafe {
                std::slice::from_raw_parts(*addr as *const u8, *size).to_vec()
            };
            let hex = bytes_to_hex(&bytes);
            let stem = sanitize_filename(name);
            let ir_path = self.scratch_dir.join(format!("{}.ll", stem));
            run_tool(
                tools.remill_lift,
                &[
                    "--arch",
                    arch_tag,
                    "--os",
                    host_os_tag(),
                    "--address",
                    &format!("0x{:x}", lift_addr),
                    "--entry_address",
                    &format!("0x{:x}", lift_addr),
                    "--bytes",
                    &hex,
                    "--ir_out",
                    ir_path.to_str().expect("scratch path is utf-8"),
                ],
                name,
            )?;
            let obj_path = self.scratch_dir.join(format!("{}.o", stem));
            run_tool(
                tools.llc,
                &[
                    "-mtriple=wasm32-unknown-unknown",
                    "-filetype=obj",
                    "-O2",
                    "-o",
                    obj_path.to_str().expect("scratch path is utf-8"),
                    ir_path.to_str().expect("scratch path is utf-8"),
                ],
                name,
            )?;
            exports.push(remill_export_symbol(lift_addr));
            object_paths.push(obj_path);
        }

        let wasm_path = self.scratch_dir.join("azul-mini.wasm");
        let mut wasm_ld_args: Vec<String> = vec![
            "--no-entry".to_string(),
            "--allow-undefined".to_string(),
            "-o".to_string(),
            wasm_path.to_string_lossy().into_owned(),
        ];
        for e in &exports {
            wasm_ld_args.push(format!("--export={}", e));
        }
        for p in &object_paths {
            wasm_ld_args.push(p.to_string_lossy().into_owned());
        }
        let arg_refs: Vec<&str> = wasm_ld_args.iter().map(String::as_str).collect();
        run_tool(tools.wasm_ld, &arg_refs, "azul-mini")?;

        let wasm_bytes = std::fs::read(&wasm_path).map_err(|e| TranspileError {
            fn_name: "azul-mini".into(),
            reason: format!("read {}: {e}", wasm_path.display()),
        })?;

        Ok(WasmModule {
            content_hash: super::fnv1a64_hex(&wasm_bytes),
            bytes: wasm_bytes,
            exports,
            imports_from_mini: Vec::new(),
            used_boundaries: Vec::new(),
        })
    }

    fn lift_and_link_eventloop(
        &self,
        symbols: &[(String, usize, usize)],
    ) -> Result<WasmModule, TranspileError> {
        if symbols.is_empty() {
            return Err(TranspileError {
                fn_name: "azul-mini".into(),
                reason: "no eventloop symbols provided".into(),
            });
        }
        // Verify all symbols have a known signature before doing any
        // expensive lift work. This fails fast on a typo in
        // EVENTLOOP_SYMBOLS that doesn't have a matching entry in
        // signature_for_eventloop_fn.
        //
        // M11 Sprint 1: each eventloop fn becomes a TransitiveLiftRoot
        // so its transitive Rust deps (e.g. cascade machinery called
        // from `AzStartup_hydrateStyledDom`) get lifted too. The old
        // per-fn-only path stubbed every non-eventloop call to noop
        // via the JS Proxy, which made the marker-field hydrate a
        // dead-end. The transitive lifter walks the dep graph via
        // ARM64 bytes-scan and pulls every Recursable callee into
        // the same wasm.
        let mut roots: Vec<TransitiveLiftRoot> = Vec::with_capacity(symbols.len());
        for (name, addr, size) in symbols {
            let sig = signature_for_eventloop_fn(name).ok_or_else(|| TranspileError {
                fn_name: name.clone(),
                reason: format!(
                    "no entry in signature_for_eventloop_fn for {} — add it before \
                     listing in EVENTLOOP_SYMBOLS",
                    name
                ),
            })?;
            roots.push(TransitiveLiftRoot {
                fn_name: name.clone(),
                fn_addr: *addr,
                fn_size: *size,
                sig,
                export_as: name.clone(),
                extra_exports: Vec::new(),
            });
        }

        // M10-C1: hand-written bump-helpers exposing
        // AzStartup_snapshotBumpHeap + AzStartup_resetBumpHeap so JS
        // can clamp the wasm bump heap between cycles. These bypass
        // the lift pipeline (no native code to lift — they're
        // structurally a single load/store on @__az_bump_ptr). The
        // transitive lifter takes them as `extra_objects` and adds
        // their exports to the wasm-ld `--export` list via
        // `extra_exports_passthrough`.
        //
        // M11 Sprint 1: scratch dir creation moved here because the
        // old per-fn lift path created it inside `produce_object_for`
        // before `emit_bump_helpers_object` ran. The new transitive
        // path runs the lifter AFTER bump_helpers, so we ensure the
        // dir exists upfront.
        std::fs::create_dir_all(&self.scratch_dir).map_err(|e| TranspileError {
            fn_name: "azul-mini".into(),
            reason: format!("scratch dir: {e}"),
        })?;
        let bump_obj_path = self.emit_bump_helpers_object()?;

        // M11 Sprint 1: 4096-fn recursion cap. The per-cb default of
        // 256 is enough for the layout cb's ~141-fn dep set, but
        // cascade (`StyledDom::create` →
        // `restyle/apply_ua_css/compute_inherited_values/...`) plus
        // layout solver are estimated at 500-2000 fns each. 4096
        // gives plenty of headroom; runaway recursion still trips
        // the cap.
        let opts = LiftOpts {
            extra_objects: vec![bump_obj_path],
            extra_exports_passthrough: vec![
                "AzStartup_snapshotBumpHeap".to_string(),
                "AzStartup_resetBumpHeap".to_string(),
            ],
            memory_mode: MemoryMode::OwnMemory,
            // M12.9: AZ_MINI_MAX_DEPTH=<N> caps cascade transitive
            // lift at N functions. Used to bisect which deep helper
            // introduces the hydrate trap; default 4096 absorbs the
            // full cascade tree.
            max_recursive_depth: std::env::var("AZ_MINI_MAX_DEPTH")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(4096),
            output_stem: "azul-mini".to_string(),
        };

        let mut module = self.lift_with_transitive_deps_ex(roots, opts)?;
        // The transitive lifter populates `used_boundaries`
        // correctly; eventloop deps that hit a BoundaryImport
        // classification now properly route through the boundary
        // shard wiring. Pre-refactor this was always `vec![]`
        // because the per-fn lift didn't walk deps.
        if !module.used_boundaries.is_empty() {
            eprintln!(
                "[azul-web]   eventloop: discovered {} boundary imports via transitive lift",
                module.used_boundaries.len(),
            );
        }
        // Preserve the original `imports_from_mini: Vec::new()`
        // shape — mini doesn't import from itself.
        module.imports_from_mini = Vec::new();
        Ok(module)
    }

    fn is_available(&self) -> bool {
        self.remill_lift.is_some()
            && self.llc.is_some()
            && self.opt.is_some()
            && self.llvm_link.is_some()
            && self.wasm_ld.is_some()
    }

    fn name(&self) -> &str {
        "RemillTranspiler (remill-lift-17 + llc + wasm-ld subprocess)"
    }
}

// M10 inherent impl. Carries the hand-written bump-helper IR + the
// merged-compile transitive-lift path. Kept separate from the
// `impl Transpiler` block because these aren't part of the public
// Transpiler trait surface.
impl RemillTranspiler {
    /// M10-D — lift one boundary symbol into its own per-fn wasm
    /// shard. The shard exports `sub_<canonical_hex>` (the raw lifted
    /// body using the standard remill signature
    /// `ptr (ptr state, i64 pc, ptr memory)`) so other wasms can
    /// import it via `env.sub_<canonical_hex>` and call it directly
    /// from their own lifted bodies.
    ///
    /// Internally reuses [`Self::lift_with_transitive_deps_batched`]:
    /// the BFS walks the boundary's own dep graph and pulls every
    /// Recursable dep into the same shard. Other [`FnClass::BoundaryImport`]
    /// deps stay as env-imports — they ship as their own shards and
    /// JS wires them at instantiate-time.
    ///
    /// The root's `export_as` uses the `AzBoundary_<hex>` prefix so
    /// [`produce_object_from_lifted_ir`]'s alwaysinline path is
    /// skipped (otherwise opt -O2 would inline the body into the
    /// unused wrapper and --gc-sections would then strip both, taking
    /// the exported `sub_<canonical_hex>` body with them).
    pub fn lift_boundary_to_wasm(
        &self,
        boundary_addr: usize,
    ) -> Result<BoundaryShard, TranspileError> {
        let table = symbol_table::get().ok_or_else(|| TranspileError {
            fn_name: format!("boundary_0x{:x}", boundary_addr),
            reason: "SymbolTable not installed — cannot lift boundary".into(),
        })?;
        let entry = table.lookup(boundary_addr).ok_or_else(|| TranspileError {
            fn_name: format!("boundary_0x{:x}", boundary_addr),
            reason: format!(
                "boundary canonical addr 0x{:x} not in SymbolTable",
                boundary_addr,
            ),
        })?;
        let canonical_name = entry.canonical_name.clone();
        let synth_hex = format!("{:x}", entry.synthetic_addr);
        let body_export = format!("sub_{}", synth_hex);
        let fn_size = if entry.size > 0 {
            entry.size
        } else {
            super::LIFT_READ_WINDOW
        };
        let canonical_addr = entry.canonical_addr;

        // Wrapper name uses the `AzBoundary_` prefix so the
        // alwaysinline-skip path triggers in
        // produce_object_from_lifted_ir. The `__az_dep_` infix tells
        // the per-target loop to NOT add the wrapper to the export
        // list — `--gc-sections` then strips the wrapper at link
        // time (its body is never called; `sub_<X>` is the actual
        // callable, anchored via `extra_exports`).
        let wrapper_export = format!("__az_dep_AzBoundary_{}", synth_hex);
        let root = TransitiveLiftRoot {
            fn_name: canonical_name.clone(),
            fn_addr: boundary_addr,
            fn_size,
            sig: signature_for_callback_kind("Callback"),
            export_as: wrapper_export.clone(),
            extra_exports: vec![body_export.clone()],
        };

        let module = self.lift_with_transitive_deps(vec![root])?;

        Ok(BoundaryShard {
            canonical_addr,
            canonical_name,
            body_export,
            wrapper_export,
            content_hash: module.content_hash,
            wasm_bytes: module.bytes,
            transitive_boundaries: module.used_boundaries,
        })
    }

    /// M10-B1.b — prepare (patched_ir, helper_ir) strings for one
    /// lifted function WITHOUT compiling or writing .o files. Used by
    /// the merged-compile path
    /// ([`compile_merged_transitive_object`]) which feeds multiple
    /// per-fn IRs to a single `compile_to_wasm32_obj` so opt -O2 can
    /// inline + SROA across the whole dep graph.
    ///
    /// `tag_with_alwaysinline_all`: when true, inject `alwaysinline`
    /// on EVERY `define ptr @sub_<hex>(...) {` in the patched IR
    /// (not just the entry). For merged compile this is the lever
    /// that unblocks State-alloca SROA.
    fn prepare_per_fn_irs(
        &self,
        fn_name: &str,
        fn_addr: usize,
        lift_addr: u64,
        sig: &CallbackSignature,
        export_as: &str,
        raw_lifted_ir: &str,
        tag_with_alwaysinline_all: bool,
    ) -> Result<(String, String), TranspileError> {
        std::fs::create_dir_all(&self.scratch_dir).map_err(|e| TranspileError {
            fn_name: fn_name.to_string(),
            reason: format!("scratch dir: {e}"),
        })?;
        let stem = sanitize_filename(export_as);
        let _ = std::fs::write(
            self.scratch_dir.join(format!("{}_{:x}.lifted.ll", sanitize_filename(fn_name), fn_addr)),
            raw_lifted_ir,
        );

        let lifted_ir = match symbol_table::get() {
            Some(table) => {
                let rewritten =
                    rewrite_sub_names_to_canonical(raw_lifted_ir, table, fn_addr, lift_addr);
                dedup_sub_declares(&rewritten)
            }
            None => raw_lifted_ir.to_string(),
        };
        let lifted_ir = tag_state_accesses(&lifted_ir);
        // M12.5d: see notes at the other call site — retarget the
        // AArch64 IR header to wasm32 before opt/llc runs.
        let lifted_ir = retarget_to_wasm32(&lifted_ir);
        // M12.5d-fix: strip noalias from sub_* args (see fn docs).
        let lifted_ir = strip_noalias_from_sub_args(&lifted_ir);

        let canonical_entry_addr = symbol_table::get()
            .and_then(|t| {
                t.lookup(fn_addr)
                    .map(|e| t.resolve_synth(e.synthetic_addr).unwrap_or(e.synthetic_addr))
            })
            .unwrap_or(fn_addr) as u64;

        let patched_ir = if export_as.starts_with("AzStartup_")
            || export_as.contains("AzBoundary_")
        {
            // Same rationale as produce_object_from_lifted_ir:
            // AzStartup_* and AzBoundary_* roots can't tolerate
            // alwaysinline (call-observer gets DCE'd in the AzStartup
            // case; boundary's exported `sub_<X>` body gets stripped
            // in the AzBoundary case).
            lifted_ir.clone()
        } else if tag_with_alwaysinline_all {
            inject_alwaysinline_all_subs(&lifted_ir)
        } else {
            inject_alwaysinline(&lifted_ir, canonical_entry_addr)
        };
        let patched_ir_path = self.scratch_dir.join(format!("{}.patched.ll", stem));
        std::fs::write(&patched_ir_path, &patched_ir).map_err(|e| TranspileError {
            fn_name: fn_name.to_string(),
            reason: format!("write patched IR: {e}"),
        })?;

        let branch_sym_names = parse_extern_sub_declares(&lifted_ir);
        let mut resolved_branches: Vec<ResolvedBranchExtern> =
            Vec::with_capacity(branch_sym_names.len());
        for sym_name in &branch_sym_names {
            let synth_addr = parse_sub_hex_as_addr(sym_name).unwrap_or(0);
            let classification = symbol_table::get()
                .and_then(|t| t.lookup_by_synth(synth_addr))
                .map(|e| e.classification);
            resolved_branches.push(ResolvedBranchExtern {
                sym_name: sym_name.clone(),
                classification,
            });
        }
        let helper_ir = emit_helper_ir(
            canonical_entry_addr,
            sig,
            &resolved_branches,
            export_as,
        );
        let helper_ir = tag_state_accesses(&helper_ir);
        let helper_ir_path = self.scratch_dir.join(format!("{}.helper.ll", stem));
        std::fs::write(&helper_ir_path, &helper_ir).map_err(|e| TranspileError {
            fn_name: fn_name.to_string(),
            reason: format!("write helper IR: {e}"),
        })?;

        Ok((patched_ir, helper_ir))
    }

    /// M10-B1.b — compile a batch of (patched_ir, helper_ir) pairs
    /// into ONE wasm32 object via merged linkInModule + opt -O2.
    /// All per-fn IRs become a single module before opt runs, so
    /// alwaysinline-marked functions get inlined into their callers
    /// across what used to be `.o` boundaries.
    ///
    /// Returns the path of the resulting `.o` file.
    ///
    /// Only available with the native compile path. Returns an error
    /// otherwise (the subprocess path would need a multi-input
    /// llvm-link invocation which doesn't help latency anyway).
    fn compile_merged_transitive_object(
        &self,
        ir_pairs: &[(String, String)],
    ) -> Result<PathBuf, TranspileError> {
        if !self.use_native_remill() {
            return Err(TranspileError {
                fn_name: "transitive-merged".into(),
                reason: "merged compile requires AZ_NATIVE_REMILL=1".into(),
            });
        }
        // Flatten (patched, helper) pairs into a single &[&str] for
        // compile_to_wasm32_obj. parseIR runs once per input, then
        // linkInModule merges them all into the first.
        let mut all_irs: Vec<&str> = Vec::with_capacity(ir_pairs.len() * 2);
        for (patched, helper) in ir_pairs {
            all_irs.push(patched.as_str());
            all_irs.push(helper.as_str());
        }
        #[cfg(feature = "web-transpiler-static")]
        {
            let obj_bytes = super::native_remill::compile_to_wasm32_obj(&all_irs)
                .map_err(|e| TranspileError {
                    fn_name: "transitive-merged".into(),
                    reason: format!("native compile: {}", e),
                })?;
            let obj_path = self.scratch_dir.join("transitive_merged.o");
            std::fs::write(&obj_path, &obj_bytes).map_err(|e| TranspileError {
                fn_name: "transitive-merged".into(),
                reason: format!("write {}: {e}", obj_path.display()),
            })?;
            Ok(obj_path)
        }
        #[cfg(not(feature = "web-transpiler-static"))]
        {
            let _ = all_irs;
            Err(TranspileError {
                fn_name: "transitive-merged".into(),
                reason: "web-transpiler-static feature required for merged compile"
                    .into(),
            })
        }
    }

    /// M10-C1 — hand-written IR for the bump-heap snapshot/reset
    /// helpers. Two functions sit alongside the lifted AzStartup_* in
    /// azul-mini.wasm and expose direct load/store on the
    /// `@__az_bump_ptr` global to JS:
    ///
    ///   `AzStartup_snapshotBumpHeap() -> u32`
    ///   `AzStartup_resetBumpHeap(u32 snapshot) -> ()`
    ///
    /// The bump pointer is a `linkonce_odr` global initialized to
    /// 96 MiB in the per-fn helper IR (see [`emit_helper_ir`]'s
    /// `bump_global`). wasm-ld dedupes the linkonce_odr definitions
    /// across every object file in the link set so the `external`
    /// declaration here resolves to that single instance.
    ///
    /// JS usage:
    /// ```js
    /// // After init + hydrate + (any persistent setup):
    /// const snap = mini.AzStartup_snapshotBumpHeap();
    /// // Per cycle (after reading the cycle's return values):
    /// mini.AzStartup_resetBumpHeap(snap);
    /// ```
    fn emit_bump_helpers_object(&self) -> Result<PathBuf, TranspileError> {
        let ir = r#"; M10-C1 — bump-heap snapshot / reset helpers.
target datalayout = "e-m:e-p:32:32-p10:8:8-p20:8:8-i64:64-n32:64-S128-ni:1:10:20"
target triple = "wasm32-unknown-unknown"

@__az_bump_ptr = external global i32, align 4

define i32 @AzStartup_snapshotBumpHeap() {
  %v = load i32, ptr @__az_bump_ptr, align 4
  ret i32 %v
}

define void @AzStartup_resetBumpHeap(i32 %snapshot) {
  store i32 %snapshot, ptr @__az_bump_ptr, align 4
  ret void
}
"#;
        let obj_path = self.scratch_dir.join("bump_helpers.o");
        if self.use_native_remill() {
            #[cfg(feature = "web-transpiler-static")]
            {
                let obj_bytes = super::native_remill::compile_to_wasm32_obj(&[ir])
                    .map_err(|e| TranspileError {
                        fn_name: "bump_helpers".into(),
                        reason: format!("native compile: {}", e),
                    })?;
                std::fs::write(&obj_path, &obj_bytes).map_err(|e| TranspileError {
                    fn_name: "bump_helpers".into(),
                    reason: format!("write {}: {e}", obj_path.display()),
                })?;
                return Ok(obj_path);
            }
        }
        // Subprocess path: write .ll, opt + llc → .o.
        let ll_path = self.scratch_dir.join("bump_helpers.ll");
        std::fs::write(&ll_path, ir).map_err(|e| TranspileError {
            fn_name: "bump_helpers".into(),
            reason: format!("write {}: {e}", ll_path.display()),
        })?;
        let opt_path = self.scratch_dir.join("bump_helpers.opt.ll");
        let opt = self.opt.as_deref().ok_or_else(|| TranspileError {
            fn_name: "bump_helpers".into(),
            reason: "opt not found — set $LLVM_OPT or install LLVM 21".into(),
        })?;
        run_tool(
            opt,
            &[
                "-O2",
                "-S",
                ll_path.to_str().expect("scratch path is utf-8"),
                "-o",
                opt_path.to_str().expect("scratch path is utf-8"),
            ],
            "bump_helpers",
        )?;
        let llc = self.llc.as_deref().ok_or_else(|| TranspileError {
            fn_name: "bump_helpers".into(),
            reason: "llc not found — set $LLC or install LLVM 21".into(),
        })?;
        run_tool(
            llc,
            &[
                "-mtriple=wasm32-unknown-unknown",
                "-filetype=obj",
                "-O2",
                "-o",
                obj_path.to_str().expect("scratch path is utf-8"),
                opt_path.to_str().expect("scratch path is utf-8"),
            ],
            "bump_helpers",
        )?;
        Ok(obj_path)
    }
}

// ── helpers ─────────────────────────────────────────────────────────────

struct Tools<'a> {
    remill_lift: &'a Path,
    llc: &'a Path,
    wasm_ld: &'a Path,
}

/// Workspace root baked in at compile time via `CARGO_MANIFEST_DIR`.
/// The dll's manifest is at `<workspace>/dll/Cargo.toml`, so its
/// parent is the workspace root. Used by the discover_* helpers to
/// find the bundled `third_party/remill-install/...` regardless of
/// the running binary's cwd.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn discover_remill_lift() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("REMILL_LIFT_BIN") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    let ws = workspace_root().join(
        "third_party/remill-install/build/remill/bin/lift/remill-lift-17",
    );
    if ws.is_file() {
        return Some(ws);
    }
    let candidates = [
        // Cwd-relative — covers the historical case where the binary
        // is run from the workspace root.
        "third_party/remill-install/build/remill/bin/lift/remill-lift-17",
        // Fallback for installed remill.
        "/usr/local/bin/remill-lift-17",
        "/opt/homebrew/bin/remill-lift-17",
    ];
    for c in candidates {
        let pb = PathBuf::from(c);
        if pb.is_file() {
            return Some(pb);
        }
    }
    None
}

fn discover_llc() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("LLC") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    let candidates = [
        "/opt/homebrew/opt/llvm@21/bin/llc",
        "/opt/homebrew/opt/llvm/bin/llc",
        "/usr/local/opt/llvm@21/bin/llc",
        "/usr/local/opt/llvm/bin/llc",
        "/usr/bin/llc",
    ];
    for c in candidates {
        let pb = PathBuf::from(c);
        if pb.is_file() {
            return Some(pb);
        }
    }
    None
}

fn discover_opt() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("LLVM_OPT") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    let candidates = [
        "/opt/homebrew/opt/llvm@21/bin/opt",
        "/opt/homebrew/opt/llvm/bin/opt",
        "/usr/local/opt/llvm@21/bin/opt",
        "/usr/local/opt/llvm/bin/opt",
        "/usr/bin/opt",
    ];
    for c in candidates {
        let pb = PathBuf::from(c);
        if pb.is_file() {
            return Some(pb);
        }
    }
    None
}

fn discover_llvm_link() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("LLVM_LINK") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    let candidates = [
        "/opt/homebrew/opt/llvm@21/bin/llvm-link",
        "/opt/homebrew/opt/llvm/bin/llvm-link",
        "/usr/local/opt/llvm@21/bin/llvm-link",
        "/usr/local/opt/llvm/bin/llvm-link",
        "/usr/bin/llvm-link",
    ];
    for c in candidates {
        let pb = PathBuf::from(c);
        if pb.is_file() {
            return Some(pb);
        }
    }
    None
}

fn discover_wasm_ld() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("WASM_LD") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    let candidates = [
        "/opt/homebrew/opt/lld@21/bin/wasm-ld",
        "/opt/homebrew/opt/lld/bin/wasm-ld",
        "/usr/local/opt/lld@21/bin/wasm-ld",
        "/usr/local/opt/lld/bin/wasm-ld",
        "/usr/bin/wasm-ld",
    ];
    for c in candidates {
        let pb = PathBuf::from(c);
        if pb.is_file() {
            return Some(pb);
        }
    }
    None
}

fn discover_wasm_opt() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("WASM_OPT") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    let candidates = [
        "/opt/homebrew/bin/wasm-opt",
        "/opt/homebrew/opt/binaryen/bin/wasm-opt",
        "/usr/local/bin/wasm-opt",
        "/usr/bin/wasm-opt",
    ];
    for c in candidates {
        let pb = PathBuf::from(c);
        if pb.is_file() {
            return Some(pb);
        }
    }
    None
}

/// Run `wasm-opt -Oz --strip-debug --strip-producers --vacuum` on
/// the wasm at `input_path`. Returns the optimized bytes on
/// success, `None` if wasm-opt isn't installed or anything fails
/// (the caller falls back to the un-opt'd wasm).
///
/// `AZ_REMILL_SKIP_WASM_OPT=1` short-circuits — useful when
/// debugging codegen and you don't want wasm-opt's rewrites in
/// the way.
fn postprocess_wasm_opt(input_path: &Path, fn_name: &str) -> Option<Vec<u8>> {
    if std::env::var_os("AZ_REMILL_SKIP_WASM_OPT").is_some() {
        return None;
    }
    let wasm_opt = discover_wasm_opt()?;
    let out_path = input_path.with_extension("opt.wasm");
    let out_path_str = out_path.to_str()?;
    let in_path_str = input_path.to_str()?;
    let args: &[&str] = &[
        // The lifted code emits `memory.copy`/`memory.fill` (from the
        // LibcMemcpy `@llvm.memmove` body + memset lowering), which are
        // bulk-memory ops. Without `--enable-bulk-memory` wasm-opt rejects
        // the module ("requires bulk memory") and we fall back to un-opt'd
        // wasm. Enable it (browsers have supported bulk-memory since 2020).
        "--enable-bulk-memory",
        "-Oz",
        "--strip-debug",
        "--strip-producers",
        "--vacuum",
        in_path_str,
        "-o",
        out_path_str,
    ];
    match run_tool(&wasm_opt, args, fn_name) {
        Ok(()) => std::fs::read(&out_path).ok(),
        Err(e) => {
            eprintln!(
                "[azul-web]   wasm-opt failed for {}: {} (falling back to un-opt'd wasm)",
                fn_name, e.reason,
            );
            None
        }
    }
}

fn host_arch_tag() -> Option<&'static str> {
    if cfg!(target_arch = "aarch64") {
        Some("aarch64")
    } else if cfg!(target_arch = "x86_64") {
        Some("amd64")
    } else {
        None
    }
}

fn host_os_tag() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        // remill accepts "windows" too; default to "linux" for other unix-like
        // targets so the lift step doesn't reject the request.
        "linux"
    }
}

// `rewrite_tailcall_wrapper` deleted in M8.8 Stage 1. Bare-`b imm26`
// shims are detected at SymbolTable build time
// (`symbol_table::detect_arm64_tail_shims`) and chained to their
// target; `resolve_fn_ptr` chases the chain so the lifter sees the
// target's bytes, not the shim's, and remill emits a real body.

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

/// remill-lift emits a top-level function named `sub_<entry_addr>`.
/// Mirroring the blueprint experiment, this is what we ask wasm-ld
/// to export.
fn remill_export_symbol(entry_addr: u64) -> String {
    format!("sub_{:x}", entry_addr)
}

/// Bump when anything that changes lifted-IR output for the SAME input
/// bytes changes (the byte rewrites in `lift_fn`, the remill version,
/// the synth-address scheme). Invalidates the on-disk lift cache.
const LIFT_CACHE_VERSION: u32 = 1;

/// On-disk cache for `lift_fn`'s raw remill IR. The IR is synth-addressed
/// (stable across process restarts + dll relinks that don't change a
/// function's machine bytes), so caching it skips the expensive
/// `remill-lift-17` subprocess on re-lifts. Keyed by the (post-rewrite)
/// function bytes + the synth lift address + the cache version. Lives in
/// `$TMPDIR/az-lift-cache` (persists across server restarts; clear with
/// `rm -rf` or `AZ_LIFT_CACHE_CLEAR=1`). Disable entirely with
/// `AZ_NO_LIFT_CACHE=1`.
fn lift_cache_path(rewritten_bytes: &[u8], lift_addr: u64) -> PathBuf {
    let dir = std::env::temp_dir().join("az-lift-cache");
    let key = format!(
        "{}_{:x}_v{}",
        super::fnv1a64_hex(rewritten_bytes),
        lift_addr,
        LIFT_CACHE_VERSION
    );
    dir.join(format!("{key}.lifted.ll"))
}

/// LLVM `-O<level>` flag for the lift's `opt` + `llc` passes. Defaults to
/// `-O2`; set `AZ_OPT_LEVEL=0` (or 1/s/z) for faster-but-larger lifting
/// during debug iterations — the lift is still correct at `-O0` (only
/// `alwaysinline` is mandatory; SROA/inlining are size/speed, not
/// semantics), it just produces bigger wasm.
fn llvm_opt_flag() -> &'static str {
    use std::sync::OnceLock;
    static FLAG: OnceLock<String> = OnceLock::new();
    FLAG.get_or_init(|| match std::env::var("AZ_OPT_LEVEL").as_deref() {
        Ok("0") => "-O0".to_string(),
        Ok("1") => "-O1".to_string(),
        Ok("s") | Ok("S") => "-Os".to_string(),
        Ok("z") | Ok("Z") => "-Oz".to_string(),
        _ => "-O2".to_string(),
    })
    .as_str()
}

/// Per-fn opt level. When `AZ_LOWOPT_FNS` (comma-separated stems) matches the
/// fn, use `-O0` instead of the global level — so an over-aggressive opt fold
/// (e.g. proving a lifted PC-threaded loop's exit unreachable and deleting its
/// body → infinite self-loop) is avoided for that one fn, lifting it faithfully.
/// Bigger/slower wasm for that fn, but correct. Empty/unset → global level.
fn opt_flag_for(fn_name: &str) -> &'static str {
    use std::sync::OnceLock;
    static LOW: OnceLock<Vec<String>> = OnceLock::new();
    let low = LOW.get_or_init(|| {
        std::env::var("AZ_LOWOPT_FNS")
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    });
    if low.iter().any(|s| fn_name.contains(s)) {
        "-O0"
    } else {
        llvm_opt_flag()
    }
}

fn sanitize_filename(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    // Bound the length. Deeply-nested-generic Rust mangled names — e.g.
    // `<impl From<&RawGlyph<()>> for RawGlyph<SyriacData>>::from` — sanitize
    // to 190-240+ chars. With a `.lifted.ll` / `.patched.ll` / `.opt.ll`
    // suffix this exceeds the 255-byte filesystem NAME_MAX, so remill-lift-17
    // (and our own writes) hit ENAMETOOLONG; remill's `StoreModuleIRToFile`
    // then `LOG(FATAL)`s ("File name too long") → SIGABRT → the whole
    // azul-mini.wasm falls back to an 8-byte stub. Truncate to a safe prefix
    // and append a stable 64-bit FNV-1a hash of the full sanitized name so
    // two long names sharing a prefix still get distinct, deterministic stems
    // (the stem is recomputed to read each lift artifact back, so it must be a
    // pure function of `name`).
    const MAX_STEM: usize = 160;
    if s.len() <= MAX_STEM {
        return s;
    }
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    // Truncate on a char boundary (sanitized output is ASCII, so byte == char,
    // but keep this correct regardless).
    let mut prefix = s;
    prefix.truncate(MAX_STEM);
    format!("{}_{:016x}", prefix, h)
}

/// M12.5y — Heisenbug-proof store-address tracer.
///
/// Injects, before every `store <ty> <val>, ptr %X, ...` in the POST-OPT IR
/// of a target dep, a `ptrtoint`+`call @__az_logst(addr, id)` that records the
/// runtime store address + a per-store id into a ring buffer at wasm linear
/// address 0x41000 when the address is in the guest-stack window
/// [0x20000, 0x40000) (excludes the heap 0x6000000+ and the ~4 MiB mirror
/// pages).
///
/// Why this beats every runtime probe we tried: the corrupting `push_to`
/// element-copy slot store SHOULD target the heap (excluded by the window) but
/// at runtime lands on `self`(~0x2ee10, in-window). It therefore shows up in
/// the log *because* it is corrupt. The store addresses are fixed SSA values
/// after `opt`; only `llc` register allocation runs afterwards, and that cannot
/// change a computed address — so this observes the real (corrupting) store
/// without perturbing the AArch64 lift (the source of the Heisenbug in every
/// Rust-source-level probe). The emitted id maps each in-window store back to
/// its exact `.opt.ll` line (saved as `<stem>.instr.ll`), which traces through
/// SSA to the offending register (`%X27`/`%X8`/...).
/// M12.7 loop-fuel hang-finder: insert `call void @__az_fuel()` before
/// every terminator in the post-opt IR, and append an internal
/// `__az_fuel` that bumps a global tick (at 0x40068) and traps
/// (`unreachable`) once it exceeds the fuel limit (default 200M, override
/// with AZ_FUEL_LIMIT). An infinite loop runs its block's terminator
/// unboundedly → the tick overflows → trap; with AZ_WASM_DEBUG the trap's
/// named stack pinpoints the looping fn. Inserted before (never after) a
/// terminator and after any block-leading PHIs, so it is CFG/SSA-safe.
/// Rewrite empty infinite self-loops (`LABEL:` immediately followed by only
/// `br label %LABEL`) into `unreachable`. remill lifts `b .` / abort-spin
/// instructions — and opt can fold a real loop's exit away — into a block that
/// branches only to itself, which HANGS the wasm. In the lifted layout/cascade
/// those are abort / unreachable paths, so trapping is correct and far faster to
/// debug than a 120 s hang. Only empty self-loops are touched (a real loop has a
/// body between its label and its back-edge). Returns (rewritten_ir, count).
/// M12.7 diagnostic: for each conditional branch into an empty self-loop
/// (`br i1 %c, .., %SELFLOOP` where %c = `icmp eq i64 %v, 0`), insert
/// `store volatile i64 %v, 0x40078` before the branch. Only the LIVE branch's
/// store executes, so a post-trap peek of 0x40078 reveals the non-zero value
/// opt folded the loop-exit on.
fn inject_selfloop_value_log(opt_ir: &str) -> (String, u32) {
    let lines: Vec<&str> = opt_ir.lines().collect();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'$' | b'-');
    let mut selfloops: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for i in 0..lines.len() {
        let l = lines[i];
        if l.starts_with(char::is_whitespace) {
            continue;
        }
        if let Some((lbl, _)) = l.split_once(':') {
            if !lbl.is_empty()
                && lbl.bytes().all(is_ident)
                && i + 1 < lines.len()
                && lines[i + 1].trim() == format!("br label %{}", lbl)
            {
                selfloops.insert(lbl);
            }
        }
    }
    if selfloops.is_empty() {
        return (opt_ir.to_string(), 0);
    }
    let mut out = String::with_capacity(opt_ir.len() + 512);
    let mut cmp_v: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    let mut n = 0u32;
    for l in &lines {
        let t = l.trim();
        if let Some(eq) = t.find(" = icmp ") {
            let cname = &t[..eq];
            let toks: Vec<&str> = t[eq + 8..].split_whitespace().collect();
            if toks.len() >= 3 && toks[1] == "i64" {
                let op1 = toks[2].trim_end_matches(',');
                if op1.starts_with('%') && cname.starts_with('%') {
                    cmp_v.insert(cname, op1);
                }
            }
        }
        if t.starts_with("br i1 ") {
            let after = &t[6..];
            let c = after.split(',').next().map(str::trim).unwrap_or("");
            let mut hit = false;
            for (idx, _) in after.match_indices("label %") {
                let s = &after[idx + 7..];
                let end = s.bytes().position(|b| !is_ident(b)).unwrap_or(s.len());
                if selfloops.contains(&s[..end]) {
                    hit = true;
                    break;
                }
            }
            if hit {
                if let Some(v) = cmp_v.get(c) {
                    out.push_str(&format!(
                        "  store volatile i64 {}, ptr inttoptr (i64 262264 to ptr), align 8\n",
                        v
                    ));
                    n += 1;
                }
            }
        }
        out.push_str(l);
        out.push('\n');
    }
    (out, n)
}

fn rewrite_empty_self_loops(opt_ir: &str) -> (String, u32) {
    let lines: Vec<&str> = opt_ir.lines().collect();
    let mut out = String::with_capacity(opt_ir.len());
    let mut n = 0u32;
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        // Block label: a non-indented `LABEL:` (LLVM identifier), optionally
        // followed by a `; preds = ...` comment.
        let is_label = !line.starts_with(char::is_whitespace)
            && line.split_once(':').is_some_and(|(lbl, _)| {
                !lbl.is_empty()
                    && lbl.bytes().all(|b| {
                        b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'$' | b'-')
                    })
            });
        if is_label {
            let lbl = line.split_once(':').unwrap().0;
            if i + 1 < lines.len() && lines[i + 1].trim() == format!("br label %{}", lbl) {
                out.push_str(line);
                out.push('\n');
                out.push_str("  unreachable\n");
                n += 1;
                i += 2;
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
        i += 1;
    }
    (out, n)
}

fn inject_fuel(opt_ir: &str) -> (String, u32) {
    // GLOBAL terminator id, unique across ALL fueled fns (AZ_FUEL=ALL). The
    // trap records this gid at 0x40070; grep the saved `*.fuel.ll` files for
    // `@__az_fuel(i32 <gid>)` to find WHICH fn+block looped.
    static FUEL_GID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let limit: u64 = std::env::var("AZ_FUEL_LIMIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200_000_000);
    let mut out = String::with_capacity(opt_ir.len() + opt_ir.len() / 4);
    let mut n: u32 = 0;
    for line in opt_ir.lines() {
        let t = line.trim_start();
        let is_term = t.starts_with("br ")
            || t.starts_with("ret ")
            || t == "ret void"
            || t.starts_with("switch ")
            || t == "unreachable"
            || t.starts_with("indirectbr ");
        if is_term {
            let gid = FUEL_GID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            out.push_str(&format!("  call void @__az_fuel(i32 {})\n", gid));
            n += 1;
        }
        out.push_str(line);
        out.push('\n');
    }
    // 0x40068 = tick counter; 0x40060 = "fuel tripped" flag; 0x40070 = id of
    // the last block executed (= the looping block on trip). Map 0x40070 to the
    // Nth `call @__az_fuel(i32 N)` in the saved `.fuel.ll`.
    out.push_str(&format!(
        "\ndefine internal void @__az_fuel(i32 %id) {{\nentry:\n  \
         store volatile i32 %id, ptr inttoptr (i64 262256 to ptr), align 4\n  \
         %v = load i64, ptr inttoptr (i64 262248 to ptr), align 8\n  \
         %nn = add i64 %v, 1\n  \
         store i64 %nn, ptr inttoptr (i64 262248 to ptr), align 8\n  \
         %o = icmp ugt i64 %nn, {limit}\n  \
         br i1 %o, label %trap, label %ok\ntrap:\n  \
         store volatile i64 1, ptr inttoptr (i64 262240 to ptr), align 8\n  \
         unreachable\nok:\n  ret void\n}}\n"
    ));
    (out, n)
}

/// M12.7: tag each `unreachable` terminator in the post-opt IR with a
/// unique id by inserting `store volatile i64 (0x554e0000|id), ptr
/// inttoptr(0x40050)` immediately before it. `store volatile` survives
/// llc (not DCE'd), and the store executes just before the trap — so a
/// post-trap read of wasm memory at 0x40050 returns `0x554e0000|id` of the
/// LIVE (taken) unreachable. Map `id = value & 0xffff` to the Nth
/// `unreachable` in the saved `.untag.ll` to locate the opt-folded trap.
/// Opt-in via the `AZ_TAG_UNREACHABLE` env var.
fn inject_unreachable_tagging(opt_ir: &str) -> (String, u32) {
    let mut out = String::with_capacity(opt_ir.len() + 4096);
    let mut id: u32 = 0;
    for line in opt_ir.lines() {
        if line.trim_start() == "unreachable" {
            id += 1;
            let marker = 0x554e_0000_u64 | (id as u64);
            out.push_str(&format!(
                "  store volatile i64 {}, ptr inttoptr (i64 262224 to ptr), align 8\n",
                marker
            ));
        }
        out.push_str(line);
        out.push('\n');
    }
    (out, id)
}

fn inject_store_logging(opt_ir: &str, deptag: u32) -> (String, u32) {
    let mut out = String::with_capacity(opt_ir.len() + (1 << 17));
    let mut id: u32 = 0;
    for line in opt_ir.lines() {
        // Log the destination + value of a `store`, OR the dest pointer + LENGTH
        // of a memset/memcpy/memmove (a bulk zero/copy can clobber many cache
        // bytes via one call whose start address is logged elsewhere — the
        // length lets the reader detect a write that *spans* node_count).
        if let Some((dest, valdef, valop)) = parse_logged_write(line, id) {
            let indent: String = line.chars().take_while(|c| *c == ' ').collect();
            if dest == "%SP" {
                // SP-trajectory: the SP-slot store. Log with marker addr 0xF0000
                // (bypasses the window) and val = the stored SP value, so the
                // reader sees SP across every lifted call and pinpoints where it
                // is left unbalanced (the caller's SP-relative locals then break).
                if let Some(def) = valdef {
                    out.push_str(&format!("{indent}{def}\n"));
                }
                out.push_str(&format!(
                    "{indent}call void @__az_logst(i32 983040, i32 {id}, i32 {valop})\n"
                ));
            } else {
                out.push_str(&format!("{indent}%azp_{id} = ptrtoint ptr {dest} to i32\n"));
                if let Some(def) = valdef {
                    out.push_str(&format!("{indent}{def}\n"));
                }
                out.push_str(&format!(
                    "{indent}call void @__az_logst(i32 %azp_{id}, i32 {id}, i32 {valop})\n"
                ));
            }
            id += 1;
        }
        out.push_str(line);
        out.push('\n');
    }
    // Ring buffer layout (wasm linear memory):
    //   0x41000         : u32 count (total in-window writes; may exceed cap)
    //   0x41010 + k*16  : (u32 addr, u32 id, u32 deptag, u32 val) for k in 0..3500
    // val = stored value (int stores) / length (mem intrinsics) / 0xDEADBEEF
    // (non-int store) / 0xBEEF0000 (mem, no parseable len). Window
    // [0x2e000, 0x2f000) = the self/cache page (self ~0x2ee10).
    // cap*16 = 56 KiB fits 0x41010..0x4EAD0 (< on_click stack base 0x50000).
    out.push_str(&format!(
        "\ndefine internal void @__az_logst(i32 %addr, i32 %id, i32 %val) {{\n\
         azentry:\n\
         \x20 %azlo = icmp uge i32 %addr, 0\n\
         \x20 %azhi = icmp ult i32 %addr, 196608\n\
         \x20 %azwin = and i1 %azlo, %azhi\n\
         \x20 %azsp = icmp eq i32 %addr, 983040\n\
         \x20 %azin = or i1 %azwin, %azsp\n\
         \x20 br i1 %azin, label %azdo, label %azout\n\
         azdo:\n\
         \x20 %azcntp = inttoptr i32 266240 to ptr\n\
         \x20 %azcnt = load volatile i32, ptr %azcntp, align 4\n\
         \x20 %azcnt1 = add i32 %azcnt, 1\n\
         \x20 store volatile i32 %azcnt1, ptr %azcntp, align 4\n\
         \x20 %azfull = icmp uge i32 %azcnt, 3500\n\
         \x20 br i1 %azfull, label %azout, label %azwr\n\
         azwr:\n\
         \x20 %azoff = mul i32 %azcnt, 16\n\
         \x20 %azea = add i32 %azoff, 266256\n\
         \x20 %azep = inttoptr i32 %azea to ptr\n\
         \x20 store volatile i32 %addr, ptr %azep, align 4\n\
         \x20 %azida = add i32 %azea, 4\n\
         \x20 %azidp = inttoptr i32 %azida to ptr\n\
         \x20 store volatile i32 %id, ptr %azidp, align 4\n\
         \x20 %azdta = add i32 %azea, 8\n\
         \x20 %azdtp = inttoptr i32 %azdta to ptr\n\
         \x20 store volatile i32 {deptag}, ptr %azdtp, align 4\n\
         \x20 %azva = add i32 %azea, 12\n\
         \x20 %azvp = inttoptr i32 %azva to ptr\n\
         \x20 store volatile i32 %val, ptr %azvp, align 4\n\
         \x20 br label %azout\n\
         azout:\n\
         \x20 ret void\n\
         }}\n"
    ));
    (out, id)
}

/// Decide whether `line` is a tracked write and, if so, return
/// `(dest_ptr_name, optional i32-value-materialization line, i32-value-operand)`.
/// Tracks `store iN`/`store <other>` and `memset/memcpy/memmove`.
fn parse_logged_write(line: &str, id: u32) -> Option<(String, Option<String>, String)> {
    let t = line.trim_start();
    if t.starts_with("store ") {
        let dest = parse_store_dest(line)?;
        let body = t.strip_prefix("store ").unwrap_or(t);
        let body = body.strip_prefix("volatile ").unwrap_or(body);
        for (ty, kind) in [("i64", 'T'), ("i32", 'O'), ("i16", 'Z'), ("i8", 'Z')] {
            if let Some(rest) = body.strip_prefix(&format!("{ty} ")) {
                if let Some(end) = rest.find(", ptr ") {
                    let v = rest[..end].trim();
                    let def = match kind {
                        'T' => format!("%azv_{id} = trunc i64 {v} to i32"),
                        'O' => format!("%azv_{id} = or i32 {v}, 0"),
                        _ => format!("%azv_{id} = zext {ty} {v} to i32"),
                    };
                    return Some((dest, Some(def), format!("%azv_{id}")));
                }
            }
        }
        // non-int store (vector / ptr / float): log addr but a sentinel value.
        return Some((dest, None, "3735928559".to_string())); // 0xDEADBEEF
    }
    if let Some(dest) = parse_memintrinsic_dest(line) {
        if let Some(len) = parse_memintrinsic_len(line) {
            return Some((
                dest,
                Some(format!("%azv_{id} = trunc i64 {len} to i32")),
                format!("%azv_{id}"),
            ));
        }
        return Some((dest, None, "3203399168".to_string())); // 0xBEEF0000
    }
    None
}

/// Extract the destination pointer `%name` of an `@llvm.memset/memcpy/memmove`
/// (or bare host `@memset/@memcpy/@memmove`) call — the first `%`-operand after
/// `(` (the dest is always the first pointer arg).
fn parse_memintrinsic_dest(line: &str) -> Option<String> {
    let t = line.trim_start();
    let is_mem = t.contains("@llvm.memset")
        || t.contains("@llvm.memcpy")
        || t.contains("@llvm.memmove")
        || t.contains("@memcpy(")
        || t.contains("@memset(")
        || t.contains("@memmove(");
    if !is_mem {
        return None;
    }
    let open = line.find('(')?;
    let rest = &line[open + 1..];
    let pct = rest.find('%')?;
    let name: String = rest[pct..]
        .chars()
        .take_while(|c| {
            *c == '%' || c.is_ascii_alphanumeric() || matches!(*c, '.' | '_' | '-' | '$')
        })
        .collect();
    if name.len() > 1 {
        Some(name)
    } else {
        None
    }
}

/// Extract the byte-length operand (`i64 <len>`) of a memset/memcpy/memmove —
/// the first `i64` argument (dest/src are `ptr`).
fn parse_memintrinsic_len(line: &str) -> Option<String> {
    let open = line.find('(')?;
    let args = &line[open + 1..];
    let pos = args.find(" i64 ")?;
    let rest = &args[pos + 5..];
    let len: String = rest
        .chars()
        .take_while(|c| *c == '%' || c.is_ascii_alphanumeric() || matches!(*c, '.' | '_' | '-' | '$'))
        .collect();
    if len.is_empty() {
        None
    } else {
        Some(len)
    }
}

/// M12.5y FIX — enforce the AArch64 ABI invariant that SP is **callee-preserved**
/// across calls. Some lifted functions (notably `CssProperty::clone`, a large
/// multi-exit `match`) execute their prologue `sub sp,#N` but the remill lift
/// never emits the matching epilogue `add sp,#N` on the taken return path (its
/// `.patched.ll` has only the prologue SP store). Each such call therefore LEAKS
/// N bytes of guest SP; called in a loop (apply_ua_css clones ~24 properties),
/// the caller's SP drifts steadily down, corrupting its SP-relative locals — the
/// `create_from_compact_dom` cache-base = NULL / cascade-all-zero bug.
///
/// Fix: wrap every lifted `call ptr @sub_<hex>(ptr %state, ...)` so the caller
/// reloads `State.SP` (byte offset 1040 in the remill AArch64 State) before the
/// call and stores it back after — making SP preservation hold regardless of the
/// callee's epilogue lift. This is the ABI-correct invariant (no callee may
/// change its caller's SP), so it cannot mask a real bug; it only repairs leaks.
fn enforce_sp_preservation(opt_ir: &str) -> (String, u32) {
    // remill AArch64 State byte offsets of the callee-preserved registers:
    // X19..X28 (848..992 step 16), X29/FP (1008), SP (1040). X30/LR (1024) is
    // NOT preserved (link register). Saving/restoring these around a call is the
    // ABI invariant; it repairs any callee whose lifted epilogue drops them.
    const CS_OFFSETS: [u32; 12] = [848, 864, 880, 896, 912, 928, 944, 960, 976, 992, 1008, 1040];
    let mut out = String::with_capacity(opt_ir.len() + (1 << 16));
    let mut k: u32 = 0;
    for line in opt_ir.lines() {
        if let Some((_res, state_arg)) = parse_sub_call(line) {
            let indent: String = line.chars().take_while(|c| *c == ' ').collect();
            for (j, off) in CS_OFFSETS.iter().enumerate() {
                out.push_str(&format!(
                    "{indent}%azg_{k}_{j} = getelementptr inbounds i8, ptr {state_arg}, i32 {off}\n\
                     {indent}%azv_{k}_{j} = load i64, ptr %azg_{k}_{j}, align 8\n"
                ));
            }
            // Emit the call with `tail ` stripped (stores follow it now).
            out.push_str(&line.replacen("tail call ptr @sub_", "call ptr @sub_", 1));
            out.push('\n');
            for j in 0..CS_OFFSETS.len() {
                out.push_str(&format!(
                    "{indent}store i64 %azv_{k}_{j}, ptr %azg_{k}_{j}, align 8\n"
                ));
            }
            k += 1;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    (out, k)
}

/// Parse a lifted call `%N = [tail ]call ptr @sub_<hex>(ptr [nonnull ]%S, ...`,
/// returning `(result_ssa, state_arg_ssa)`. `None` for non-`@sub_` calls.
fn parse_sub_call(line: &str) -> Option<(String, String)> {
    let t = line.trim_start();
    let res_end = t.find(" = ")?;
    let res = &t[..res_end];
    if !res.starts_with('%') {
        return None;
    }
    let rest = &t[res_end + 3..];
    let rest = rest.strip_prefix("tail ").unwrap_or(rest);
    let rest = rest.strip_prefix("call ptr @sub_")?;
    let paren = rest.find('(')?;
    let args = &rest[paren + 1..];
    let args = args.strip_prefix("ptr ").unwrap_or(args);
    let args = args.strip_prefix("nonnull ").unwrap_or(args);
    if !args.starts_with('%') {
        return None;
    }
    let name: String = args
        .chars()
        .take_while(|c| *c == '%' || c.is_ascii_alphanumeric() || matches!(*c, '.' | '_' | '-' | '$'))
        .collect();
    if name.len() > 1 {
        Some((res.to_string(), name))
    } else {
        None
    }
}

/// Extract the destination `%name` of an LLVM `store` line, or `None` if the
/// line is not a store-to-named-pointer. The destination is the operand after
/// the first `, ptr %` (the value operand, if itself a pointer, is preceded by
/// `store ` not `, `, so the first `, ptr %` is always the destination).
fn parse_store_dest(line: &str) -> Option<String> {
    if !line.trim_start().starts_with("store ") {
        return None;
    }
    let pos = line.find(", ptr %")?;
    let start = pos + ", ptr ".len(); // points at '%'
    // LLVM unquoted local identifiers are [-a-zA-Z$._][-a-zA-Z$._0-9]* — note
    // `-` (e.g. `%p.i972.pre-phi`) and `$` are valid and must be included or
    // the name is silently truncated to a non-existent value.
    let name: String = line[start..]
        .chars()
        .take_while(|c| {
            *c == '%' || c.is_ascii_alphanumeric() || matches!(*c, '.' | '_' | '-' | '$')
        })
        .collect();
    if name.len() > 1 {
        Some(name)
    } else {
        None
    }
}

/// Patch the lifted IR so the top-level `sub_<entry>` function carries
/// the `alwaysinline` attribute. Without this, opt -O2's inliner won't
/// pull the lifted body into the M6 `callback` wrapper and SROA can't
/// promote the wrapper's `[4096 x i8]` State alloca.
///
/// The patch is a single-line rewrite — remill emits each function
/// definition as `define ptr @sub_<hex>(<args>) {`; this finds the
/// opening line for `sub_<lift_addr>` and rewrites it to
/// `define ptr @sub_<hex>(<args>) alwaysinline {`. Other `sub_<hex>`
/// declarations (the branch destinations outside the byte map remill
/// saw) are left alone — they stay as `declare`s and get linked in
/// later by M7's intercept pass.
fn inject_alwaysinline(ir: &str, lift_addr: u64) -> String {
    let needle = format!("define ptr @sub_{:x}(", lift_addr);
    let mut out = String::with_capacity(ir.len() + 32);
    for line in ir.lines() {
        if line.starts_with(&needle) && line.ends_with(") {") {
            // Insert `alwaysinline` between `)` and `{`.
            let without_brace = &line[..line.len() - 1]; // strip trailing `{`
            out.push_str(without_brace.trim_end());
            out.push_str(" alwaysinline {");
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// M10-B1.b variant — inject `alwaysinline` on EVERY
/// `define ptr @sub_<hex>(...) {` line, not just the entry. Used by
/// the merged-compile transitive-lift path so opt -O2 can inline the
/// entire dep call graph into the entry wrapper. State alloca then
/// has no escape via call → SROA promotes it to registers.
///
/// Recursion check: the lift's BFS dep enumeration assumes the call
/// graph is a DAG (cycles cause MAX_RECURSIVE_DEPTH exhaustion).
/// Any actual recursive cycle in the user binary's lifted code would
/// reach this function and cause an LLVM error at inline time. The
/// caller is responsible for not landing here with a cyclic graph.
fn inject_alwaysinline_all_subs(ir: &str) -> String {
    let mut out = String::with_capacity(ir.len() + 4 * 1024);
    for line in ir.lines() {
        // Match `define ptr @sub_<hex>(... ) {` with no existing
        // attribute keywords between `)` and `{` (a defensive check
        // for already-tagged lines from re-running the pass).
        if let Some(after_sig) = line.find(") {") {
            let prefix = &line[..after_sig];
            let suffix = &line[after_sig..]; // ") {"
            let trimmed = prefix.trim_start();
            if trimmed.starts_with("define ptr @sub_")
                && !prefix.contains(" alwaysinline")
            {
                out.push_str(prefix);
                out.push_str(") alwaysinline {");
                // Skip the original `) {` since we just wrote
                // `) alwaysinline {`.
                let _ = suffix;
                out.push('\n');
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Emit the M6 helper module. Contains:
///
///   1. `__remill_*` definitions with real bodies — memory ops
///      become actual `load`/`store`, control intrinsics (function
///      return, jump, missing block, error) thread the memory token
///      through unchanged, barriers are noops. All marked
///      `alwaysinline` so opt -O2 inlines them at every call site.
///
///   2. A `callback` wrapper that allocates a separate buffer for
///      the State struct AND a separate stack-scratch buffer. The
///      split lets SROA promote State (no `ptrtoint` ever taken of
///      it) while the stack-scratch buffer absorbs the lifted body's
///      SP-relative spills.
///
/// Wrapper signature: `(i64, i64, i32) -> i32` per the AArch64 PCS
/// for `extern "C" fn(AzRefAny, AzCallbackInfo) -> AzUpdate`:
///
///   X0/X1 = AzRefAny `(refcount_ptr, instance_id)` — 16-byte
///           struct passed in two 64-bit regs.
///   X2    = `*const AzCallbackInfo` — the struct itself is huge
///           (~kilobytes), so the AArch64 PCS passes it by pointer
///           in X2. On wasm32 the pointer is `i32`; the wrapper
///           zero-extends to `i64` when storing into State's X2
///           slot so the lifted body's `i64`-typed register reads
///           see the right bit pattern.
///   W0    = AzUpdate (4-byte enum, low half of X0).
///
/// Note: this wrapper signature is callback-shape-specific. Other
/// callback types (`LayoutCallback` returning a struct,
/// `CheckBoxOnToggleCallback` taking an extra bool arg, …) need
/// their own signatures synthesized from `api.json`. That
/// generalization is M7 work.
///
/// Panic/unwind: the lifted body may contain code paths reaching
/// `core::panicking::panic_*` which the lift sees as another
/// `sub_<hex>` extern. The JS-side proxy noops these, so a "panic"
/// silently returns memory unchanged and control falls through to
/// whatever comes next in the lifted body. For correctness, panic
/// call sites should trap the WASM (M7+ would route a typed
/// `__az_panic` import to `() => throw new Error(...)` in JS); for
/// the demo path the noop fallback is acceptable.
// M10-B1.a metadata-ID slots. Numbers are arbitrary within each
// module — they MUST NOT collide with metadata IDs the lifted IR or
// llvm-link's own emission uses, so pick high IDs unlikely to clash.
// llvm-link uniques metadata nodes by structural content, so the
// identical `!{!"az_alias_domain"}` etc. emitted by the helper IR
// (here) and by `tag_state_accesses` in the lifted IR collapse to
// one set of nodes after link.
const AZ_DOMAIN_MD_ID: u32 = 90001;
const AZ_GUEST_SCOPE_MD_ID: u32 = 90002;
const AZ_HOST_SCOPE_MD_ID: u32 = 90003;
const AZ_GUEST_LIST_MD_ID: u32 = 90004;
const AZ_HOST_LIST_MD_ID: u32 = 90005;

fn emit_helper_ir(
    lift_addr: u64,
    sig: &CallbackSignature,
    branch_externs: &[ResolvedBranchExtern],
    export_as: &str,
) -> String {
    // SP register slot in the State struct (aarch64-specific).
    let sp_off: u64 = 1040;
    // X0 slot (where args are read + return is written).
    let x0_off: u64 = 544;
    // State alloca size — covers fields up to the SR/PC region at
    // offset ~1080. Rounded up to a multiple of 16 for alignment.
    let state_size: u64 = 1088;
    // Separate stack-scratch buffer for the lifted body's
    // SP-relative spills. The body's prologue decrements SP and
    // stores callee-saves at SP-relative addresses. Each lifted
    // function adds ~96-256 bytes of spill area. Deep libazul call
    // chains (layout cb's pipeline crosses ~50+ frames) need real
    // headroom — 4 KiB underflows SP, wraps to a huge u64, and the
    // first SP-relative load traps OOB. wasm-ld's default global
    // $stack_pointer reserves 64 KiB; making `%stack_buf` larger
    // than that would itself overflow the wasm stack as soon as
    // the wrapper enters.
    //
    // M12.8 update: with NEON Q-reg STP now lifted in our remill
    // fork, hello-world.bin's layout cb traverses a deeper dep
    // chain (StyledDom::create-style helpers, etc.). 32 KiB
    // underflows. Bump to 128 KiB — wasm-ld's stack ceiling for
    // a fresh wasm instance is well above this, and `%stack_buf`
    // lives in linear memory not the wasm function-call stack, so
    // the wasm instance's own stack budget isn't affected.
    let stack_size: u64 = 128 * 1024;
    let (params, prologue) = emit_wrapper_args_and_prologue(sig);
    let (ret_ty, ret_code) = emit_wrapper_return(sig);
    // M7: emit a body for every branch-destination extern the lift
    // surfaced. Bodies depend on the symbol's dladdr-resolved
    // classification:
    //   - Noop (default): return memory unchanged. Lift's call site
    //     reads back garbage from X0 — fine for void/error returns
    //     but breaks any caller that uses the result as a pointer.
    //   - RustAlloc / RustAllocZeroed: bump `@__bump_ptr` by X0's
    //     value (size), write the old `@__bump_ptr` value back to
    //     X0 (the returned pointer). After opt -O2 inlines this
    //     into the lift's call site, allocator-flowed pointers are
    //     real wasm32 offsets and subsequent Box::new / Vec::push
    //     / BTreeMap::insert code paths execute correctly.
    //   - RustRealloc: bump alloc fresh region of new_size, memcpy
    //     min(old_size, new_size) bytes from old, leak the old
    //     region. Required for Vec resizes in the layout-cb path.
    //   - RustDealloc: noop body (bump-only allocator doesn't free).
    //
    // `@__bump_ptr` is declared `linkonce_odr` so the wasm-ld link
    // step over multiple object files (the azul-mini eventloop
    // case) dedupes to one shared global — every AzStartup_* shares
    // the same heap. Initial offset 65536 (64 KiB) leaves the wasm
    // stack guard zone alone; subsequent grow is fine because
    // azul-mini.wasm imports `memory` from JS with growth allowed.
    let mut branch_stubs = String::new();
    for ext in branch_externs {
        // SSA-name suffix: derive from the sym_name's hex. After
        // canonicalization there's no `.N` to strip, so a single
        // address-based suffix is unique across call sites.
        let n_suffix = ext.sym_name.as_str();
        match ext.classification {
            // Recursable: NO body — the recursive walker lifts this
            // function in a sibling .o, and wasm-ld matches the
            // sub_<canonical_addr_hex> defn to the sub_<canonical_addr_hex>
            // declare automatically. Drop through to no-emission.
            Some(SymFnClass::Recursable) => {}
            // M10-D BoundaryImport: NO body either, but for a different
            // reason — there's no sibling .o either. wasm-ld sees the
            // `declare ptr @sub_<canonical_hex>(...)` as undefined and
            // (with `--allow-undefined`) emits a wasm function-import
            // for it. At instantiate time, loader.js wires
            // `env.sub_<canonical_hex>` to the matching boundary-shard
            // wasm's exported body. The boundary-lift pass runs once
            // per server start and produces one wasm per boundary.
            Some(SymFnClass::BoundaryImport) => {}
            // BumpAlloc: __rust_alloc / __rust_alloc_zeroed body.
            // The lifted Rust code expects `__rust_alloc_zeroed` to
            // return zero-init memory. wasm linear memory is
            // zero-init at startup, so a *fresh* alloc returns 0.
            // But the bump_ptr only moves forward, so a "fresh"
            // alloc is always at memory the bump never touched —
            // also zero. So in principle the `zeroed` semantics are
            // satisfied for free.
            //
            // ...except a previous call to `__rust_alloc` (non-zero)
            // also moves the bump_ptr forward, and the lifted code
            // for THAT call writes into the allocation. If the same
            // bump_ptr range is later seen by a future alloc, it
            // contains the old writes, not zero. Bump-only never
            // reuses regions, so this can't happen in steady state.
            //
            // HOWEVER, before the cascade's first alloc, the bump_ptr
            // is at @__az_bump_ptr's init value (96 MiB). Earlier
            // wasm Data segments OR wasm instructions may have
            // populated bytes in that region. To make this fully
            // correct we memset(0) the freshly allocated region.
            // Negligible cost (memset is intrinsic-fast on wasm).
            Some(SymFnClass::BumpAlloc) => {
                branch_stubs.push_str(&format!(
                    "; bump-allocator body for {sym}\n\
                     define linkonce_odr ptr @{sym}(ptr %state, i64 %pc, ptr %memory) alwaysinline {{\n  \
                       %x0_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x0_off}\n  \
                       %size_{n} = load i64, ptr %x0_p_{n}, align 8\n  \
                       store volatile i64 %size_{n}, ptr inttoptr (i64 262192 to ptr), align 8\n  \
                       %dbgc_{n} = load volatile i64, ptr inttoptr (i64 262200 to ptr), align 8\n  \
                       %dbgcp_{n} = add i64 %dbgc_{n}, 1\n  \
                       store volatile i64 %dbgcp_{n}, ptr inttoptr (i64 262200 to ptr), align 8\n  \
                       %size_a_{n} = add i64 %size_{n}, 7\n  \
                       %size_aligned_{n} = and i64 %size_a_{n}, -8\n  \
                       %old_{n} = load i32, ptr @__az_bump_ptr, align 4\n  \
                       %old_i64_{n} = zext i32 %old_{n} to i64\n  \
                       %new_i64_{n} = add i64 %old_i64_{n}, %size_aligned_{n}\n  \
                       %new_{n} = trunc i64 %new_i64_{n} to i32\n  \
                       store i32 %new_{n}, ptr @__az_bump_ptr, align 4\n  \
                       store i64 %old_i64_{n}, ptr %x0_p_{n}, align 8\n  \
                       store volatile i64 %old_i64_{n}, ptr inttoptr (i64 262208 to ptr), align 8\n  \
                       %dest_p_{n} = inttoptr i32 %old_{n} to ptr\n  \
                       call void @llvm.memset.p0.i64(ptr %dest_p_{n}, i8 0, i64 %size_aligned_{n}, i1 false)\n  \
                       ret ptr %memory\n\
                     }}\n",
                    sym = ext.sym_name,
                    n = n_suffix,
                    x0_off = x0_off,
                ));
            }
            // BumpRealloc: __rust_realloc(old_ptr, old_size, align, new_size).
            //   X0=old_ptr, X1=old_size, X2=align (ignored), X3=new_size.
            //   Returns: new_ptr in X0.
            //
            // Bump-only allocator — alloc fresh region of new_size,
            // memcpy min(old_size, new_size) bytes from old, leak the
            // old region. Vec::push past capacity (layout-cb's NodeData
            // accumulation) needs this to work.
            Some(SymFnClass::BumpRealloc) => {
                branch_stubs.push_str(&format!(
                    "; bump-realloc body for {sym}\n\
                     define linkonce_odr ptr @{sym}(ptr %state, i64 %pc, ptr %memory) alwaysinline {{\n  \
                       %x0_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x0_off}\n  \
                       %old_ptr_i64_{n} = load i64, ptr %x0_p_{n}, align 8\n  \
                       %old_ptr_i32_{n} = trunc i64 %old_ptr_i64_{n} to i32\n  \
                       %old_ptr_p_{n} = inttoptr i32 %old_ptr_i32_{n} to ptr\n  \
                       %x1_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x1_off}\n  \
                       %old_size_{n} = load i64, ptr %x1_p_{n}, align 8\n  \
                       %x3_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x3_off}\n  \
                       %new_size_{n} = load i64, ptr %x3_p_{n}, align 8\n  \
                       store volatile i64 %new_size_{n}, ptr inttoptr (i64 262192 to ptr), align 8\n  \
                       %dbgcr_{n} = load volatile i64, ptr inttoptr (i64 262200 to ptr), align 8\n  \
                       %dbgcrp_{n} = add i64 %dbgcr_{n}, 1\n  \
                       store volatile i64 %dbgcrp_{n}, ptr inttoptr (i64 262200 to ptr), align 8\n  \
                       %new_size_a_{n} = add i64 %new_size_{n}, 7\n  \
                       %new_size_aligned_{n} = and i64 %new_size_a_{n}, -8\n  \
                       %old_bump_{n} = load i32, ptr @__az_bump_ptr, align 4\n  \
                       %old_bump_i64_{n} = zext i32 %old_bump_{n} to i64\n  \
                       %new_bump_i64_{n} = add i64 %old_bump_i64_{n}, %new_size_aligned_{n}\n  \
                       %new_bump_{n} = trunc i64 %new_bump_i64_{n} to i32\n  \
                       store i32 %new_bump_{n}, ptr @__az_bump_ptr, align 4\n  \
                       %new_ptr_p_{n} = inttoptr i32 %old_bump_{n} to ptr\n  \
                       %cmp_{n} = icmp ult i64 %old_size_{n}, %new_size_{n}\n  \
                       %copy_size_{n} = select i1 %cmp_{n}, i64 %old_size_{n}, i64 %new_size_{n}\n  \
                       call void @llvm.memcpy.p0.p0.i64(ptr %new_ptr_p_{n}, ptr %old_ptr_p_{n}, i64 %copy_size_{n}, i1 false)\n  \
                       store i64 %old_bump_i64_{n}, ptr %x0_p_{n}, align 8\n  \
                       ret ptr %memory\n\
                     }}\n",
                    sym = ext.sym_name,
                    n = n_suffix,
                    x0_off = x0_off,
                    x1_off = x0_off + 16,
                    x3_off = x0_off + 48,
                ));
            }
            // LibcMemcpy: libc memcpy / memmove.
            //   X0=dest, X1=src, X2=n. Returns dest in X0.
            //
            // The real symbol is an out-of-image libsystem address
            // (PLT-chased), so it can't be lifted; the default `Leaf`
            // stub RETURNS without copying. Rust emits an out-of-line
            // `bl _memcpy` for large struct moves (`Box::new` of a
            // Vec-containing struct, slice `.to_vec()`, …), so a
            // no-op stub silently leaves the destination at its
            // zero-init bump bytes — e.g. `Box::new(styled)` of a
            // 352-byte StyledDom read back `node_data.len == 0`.
            //
            // Emit a real `@llvm.memmove` (overlap-safe superset of
            // memcpy; correct for the `_platform_memmove` spelling
            // too). X0 is left holding `dest`, which is exactly
            // memcpy/memmove's return value.
            Some(SymFnClass::LibcMemcpy) => {
                branch_stubs.push_str(&format!(
                    "; libc memcpy/memmove body for {sym} (X0=dst, X1=src, X2=n)\n\
                     define linkonce_odr ptr @{sym}(ptr %state, i64 %pc, ptr %memory) alwaysinline {{\n  \
                       %dst_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x0_off}\n  \
                       %dst_i64_{n} = load i64, ptr %dst_p_{n}, align 8\n  \
                       %dst_i32_{n} = trunc i64 %dst_i64_{n} to i32\n  \
                       %dst_{n} = inttoptr i32 %dst_i32_{n} to ptr\n  \
                       %src_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x1_off}\n  \
                       %src_i64_{n} = load i64, ptr %src_p_{n}, align 8\n  \
                       %src_i32_{n} = trunc i64 %src_i64_{n} to i32\n  \
                       %src_{n} = inttoptr i32 %src_i32_{n} to ptr\n  \
                       %nbytes_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x2_off}\n  \
                       %nbytes_{n} = load i64, ptr %nbytes_p_{n}, align 8\n  \
                       call void @llvm.memmove.p0.p0.i64(ptr %dst_{n}, ptr %src_{n}, i64 %nbytes_{n}, i1 false)\n  \
                       ret ptr %memory\n\
                     }}\n",
                    sym = ext.sym_name,
                    n = n_suffix,
                    x0_off = x0_off,
                    x1_off = x0_off + 16,
                    x2_off = x0_off + 32,
                ));
            }
            // LibcMemset: libc memset.
            //   X0=dest, X1=byte (low 8 bits), X2=n. Returns dest in X0.
            //
            // Same out-of-image problem as LibcMemcpy: the default Leaf
            // stub returns without writing. CRITICAL for hashbrown — a
            // freshly-allocated table's control bytes are set to EMPTY
            // (0xFF) via `ptr::write_bytes` = memset; a no-op leaves them
            // at the bump allocator's 0x00, so `HashMap::insert`'s probe
            // never finds an empty slot → infinite loop (the M12.7 sizing
            // hang). Emit a real `@llvm.memset`. X0 still holds dest.
            Some(SymFnClass::LibcMemset) => {
                branch_stubs.push_str(&format!(
                    "; libc memset body for {sym} (X0=dst, X1=byte, X2=n)\n\
                     define linkonce_odr ptr @{sym}(ptr %state, i64 %pc, ptr %memory) alwaysinline {{\n  \
                       %dst_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x0_off}\n  \
                       %dst_i64_{n} = load i64, ptr %dst_p_{n}, align 8\n  \
                       %dst_i32_{n} = trunc i64 %dst_i64_{n} to i32\n  \
                       %dst_{n} = inttoptr i32 %dst_i32_{n} to ptr\n  \
                       %byte_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x1_off}\n  \
                       %byte_i64_{n} = load i64, ptr %byte_p_{n}, align 8\n  \
                       %byte_i8_{n} = trunc i64 %byte_i64_{n} to i8\n  \
                       %nbytes_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x2_off}\n  \
                       %nbytes_{n} = load i64, ptr %nbytes_p_{n}, align 8\n  \
                       call void @llvm.memset.p0.i64(ptr %dst_{n}, i8 %byte_i8_{n}, i64 %nbytes_{n}, i1 false)\n  \
                       ret ptr %memory\n\
                     }}\n",
                    sym = ext.sym_name,
                    n = n_suffix,
                    x0_off = x0_off,
                    x1_off = x0_off + 16,
                    x2_off = x0_off + 32,
                ));
            }
            // BumpDealloc: __rust_dealloc(ptr, size, align). Bump-only
            // allocator doesn't free — body is a noop that returns
            // memory unchanged. X0 (ptr) is left alone (return type
            // is void; caller discards X0).
            Some(SymFnClass::BumpDealloc) => {
                branch_stubs.push_str(&format!(
                    "; bump-dealloc body for {sym} — noop (bump+leak)\n\
                     define linkonce_odr ptr @{sym}(ptr %state, i64 %pc, ptr %memory) alwaysinline {{\n  \
                       ret ptr %memory\n\
                     }}\n",
                    sym = ext.sym_name,
                ));
            }
            Some(SymFnClass::CallIndirect) => {
                // table_idx=X0(u32), refany_lo=X1(u64), refany_hi=X2(u64),
                // info_ptr=X3(u32). Returns i32 in X0.
                //
                // The LLVM wasm backend lowers `inttoptr i32 %tidx to ptr`
                // followed by an indirect `call` to wasm `call_indirect`
                // using __indirect_function_table (which wasm-ld auto-imports
                // from env when any indirect call is present).
                //
                // Same anti-DCE rationale as AzResolveCallback:
                // volatile store to @__az_call_observer after the call.
                branch_stubs.push_str(&format!(
                    "; __az_call_indirect bridge — lowers to wasm call_indirect\n\
                     define linkonce_odr ptr @{sym}(ptr %state, i64 %pc, ptr %memory) alwaysinline {{\n  \
                       %tidx_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x0_off}\n  \
                       %tidx_64_{n} = load i64, ptr %tidx_p_{n}, align 8\n  \
                       %tidx_{n} = trunc i64 %tidx_64_{n} to i32\n  \
                       %lo_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x1_off}\n  \
                       %lo_{n} = load i64, ptr %lo_p_{n}, align 8\n  \
                       %hi_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x2_off}\n  \
                       %hi_{n} = load i64, ptr %hi_p_{n}, align 8\n  \
                       %info_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x3_off}\n  \
                       %info_64_{n} = load i64, ptr %info_p_{n}, align 8\n  \
                       %info_{n} = trunc i64 %info_64_{n} to i32\n  \
                       %fn_{n} = inttoptr i32 %tidx_{n} to ptr\n  \
                       %r_{n} = call i32 %fn_{n}(i64 %lo_{n}, i64 %hi_{n}, i32 %info_{n})\n  \
                       store volatile i32 %r_{n}, ptr @__az_call_observer, align 4\n  \
                       %r_64_{n} = zext i32 %r_{n} to i64\n  \
                       store i64 %r_64_{n}, ptr %tidx_p_{n}, align 8\n  \
                       ret ptr %memory\n\
                     }}\n",
                    sym = ext.sym_name,
                    n = n_suffix,
                    x0_off = x0_off,
                    x1_off = x0_off + 16,
                    x2_off = x0_off + 32,
                    x3_off = x0_off + 48,
                ));
            }
            Some(SymFnClass::CallIndirectLayout4) => {
                // M9-3: 4-arg layout dispatch shape.
                //   table_idx=X0(u32), refany_lo=X1(u64), refany_hi=X2(u64),
                //   info_ptr=X3(u32), out_ptr=X4(u32). Returns i32 in X0.
                //
                // The called function (the layout cb wrapper) has the
                // M9-1 signature `(i64, i64, i32, i32) -> i32` (the
                // extra i32 is `out_ptr`, the caller-allocated AzDom
                // destination buffer).
                //
                // Mechanically identical to `CallIndirect` plus one
                // extra X4 load + the call sig gains one i32 arg.
                branch_stubs.push_str(&format!(
                    "; __az_call_indirect_layout4 bridge — wasm call_indirect for layout cb (M9-3)\n\
                     define linkonce_odr ptr @{sym}(ptr %state, i64 %pc, ptr %memory) alwaysinline {{\n  \
                       %tidx_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x0_off}\n  \
                       %tidx_64_{n} = load i64, ptr %tidx_p_{n}, align 8\n  \
                       %tidx_{n} = trunc i64 %tidx_64_{n} to i32\n  \
                       %lo_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x1_off}\n  \
                       %lo_{n} = load i64, ptr %lo_p_{n}, align 8\n  \
                       %hi_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x2_off}\n  \
                       %hi_{n} = load i64, ptr %hi_p_{n}, align 8\n  \
                       %info_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x3_off}\n  \
                       %info_64_{n} = load i64, ptr %info_p_{n}, align 8\n  \
                       %info_{n} = trunc i64 %info_64_{n} to i32\n  \
                       %out_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x4_off}\n  \
                       %out_64_{n} = load i64, ptr %out_p_{n}, align 8\n  \
                       %out_{n} = trunc i64 %out_64_{n} to i32\n  \
                       %fn_{n} = inttoptr i32 %tidx_{n} to ptr\n  \
                       %r_{n} = call i32 %fn_{n}(i64 %lo_{n}, i64 %hi_{n}, i32 %info_{n}, i32 %out_{n})\n  \
                       store volatile i32 %r_{n}, ptr @__az_call_observer, align 4\n  \
                       %r_64_{n} = zext i32 %r_{n} to i64\n  \
                       store i64 %r_64_{n}, ptr %tidx_p_{n}, align 8\n  \
                       ret ptr %memory\n\
                     }}\n",
                    sym = ext.sym_name,
                    n = n_suffix,
                    x0_off = x0_off,
                    x1_off = x0_off + 16,
                    x2_off = x0_off + 32,
                    x3_off = x0_off + 48,
                    x4_off = x0_off + 64,
                ));
            }
            Some(SymFnClass::ResolveCallback) => {
                // Read u64 fn_addr from State.X0. Call the JS-imported
                // @__az_resolve_callback(i64 fn_addr) -> i32. Result
                // goes into State.X0.
                //
                // The `store volatile` to @__az_call_observer is an
                // anti-DCE measure. Without it, opt -O2 treats the
                // wrapper's local State alloca as fully-tracked SSA,
                // determines the resolve call's result only flows
                // through State (which doesn't escape), and DCEs the
                // entire call chain even with `memory(readwrite)` on
                // the import declaration. A volatile store to a
                // global is observable in the IR-level memory model
                // and forces opt to keep the call.
                //
                // The `declare @__az_resolve_callback` is emitted
                // once outside the loop (see `shared_decls`) so
                // multiple resolve-call sites don't duplicate it.
                branch_stubs.push_str(&format!(
                    "; __az_resolve_callback bridge → JS-imported\n\
                     define linkonce_odr ptr @{sym}(ptr %state, i64 %pc, ptr %memory) alwaysinline {{\n  \
                       %addr_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x0_off}\n  \
                       %addr_{n} = load i64, ptr %addr_p_{n}, align 8\n  \
                       %r_{n} = call i32 @__az_resolve_callback(i64 %addr_{n})\n  \
                       store volatile i32 %r_{n}, ptr @__az_call_observer, align 4\n  \
                       %r_64_{n} = zext i32 %r_{n} to i64\n  \
                       store i64 %r_64_{n}, ptr %addr_p_{n}, align 8\n  \
                       ret ptr %memory\n\
                     }}\n",
                    sym = ext.sym_name,
                    n = n_suffix,
                    x0_off = x0_off,
                ));
            }
            // Leaf: known-not-recursable (system libs, mangled Rust
            // runtime). Emit a noop body so the symbol resolves at
            // link time — avoids `env.sub_<hex>` imports that would
            // otherwise require JS-side Proxy noops. The body
            // returns memory unchanged + zeroes State.X0 so the
            // caller reads back 0 as the return value.
            //
            // Why zero X0: leaving X0 untouched returned whatever
            // arg the caller staged into X0 before the bl — typically
            // a buffer pointer for libc helpers like snprintf /
            // memmove / strlen. Downstream code interpreting that
            // as `bytes_written` (snprintf) or `length` would memcpy
            // a multi-megabyte garbage range → OOB trap. Zeroing X0
            // makes the caller see "0 bytes / null result" which
            // downstream code typically short-circuits cleanly
            // (AzString with len=0 → empty, ptr==null → skip).
            //
            // Tradeoff: real Leaf functions that actually return
            // a meaningful value get 0 instead. Callers that
            // depend on a specific non-zero return (e.g. strlen of
            // a literal) will misbehave but won't trap. The fix is
            // to either classify the symbol differently (BumpAlloc /
            // CallIndirect / etc.) or to add a dedicated stub kind.
            Some(SymFnClass::Leaf) => {
                branch_stubs.push_str(&format!(
                    "; Leaf body for {sym} — noop with X0 zeroed (avoids garbage-return traps)\n\
                     define linkonce_odr ptr @{sym}(ptr %state, i64 %pc, ptr %memory) alwaysinline {{\n  \
                       %leaf_x0_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x0_off}\n  \
                       store i64 0, ptr %leaf_x0_p_{n}, align 8\n  \
                       ret ptr %memory\n\
                     }}\n",
                    sym = ext.sym_name,
                    n = n_suffix,
                    x0_off = x0_off,
                ));
            }
            // NeverLift: AzApp_run + other server-entry-points. Should
            // never appear in a cb body; emit a trap so we hear about
            // it loudly if it ever fires through.
            Some(SymFnClass::NeverLift) => {
                // DEBUG: record which NeverLift sym was reached (its synth
                // addr) at 0x40048 just before trapping, so a post-trap peek
                // can tell handle_alloc_error (0x37993a0) from panic_* etc.
                let nl_marker = ext
                    .sym_name
                    .strip_prefix("sub_")
                    .and_then(|h| u64::from_str_radix(h, 16).ok())
                    .unwrap_or(0xDEAD);
                branch_stubs.push_str(&format!(
                    "; NeverLift trap for {sym}\n\
                     define linkonce_odr ptr @{sym}(ptr %state, i64 %pc, ptr %memory) {{\n  \
                       store volatile i64 {marker}, ptr inttoptr (i64 262216 to ptr), align 8\n  \
                       unreachable\n\
                     }}\n",
                    sym = ext.sym_name,
                    marker = nl_marker,
                ));
            }
            // No classification: the SymbolTable didn't have this
            // address. Indicates an image we didn't enumerate, or a
            // dynamically-resolved address. Leave as extern so
            // wasm-ld emits an `env.sub_<hex>` import; the M8.8
            // verification flags this as a coverage gap.
            //
            // EXCEPTION — recursive-bl rewrite marker: when
            // `rewrite_recursive_bl` rewrites a bl-target-inside-buffer
            // to `bl <pc + 0x4000000>`, the resulting synth addr
            // appears here as an unclassified extern. Detect it
            // (addr near `lift_addr + 0x4000000`) and emit a
            // forwarding stub back to `sub_<lift_addr>` so the
            // recursive call dispatches to the same function being
            // lifted.
            None => {
                // ext.sym_name is `sub_<hex>`. Parse the address.
                let parsed_addr = ext.sym_name
                    .strip_prefix("sub_")
                    .and_then(|h| u64::from_str_radix(h, 16).ok());
                let is_recursive_marker = parsed_addr.map_or(false, |a| {
                    // The rewriter shifts the target +0x4000000 bytes
                    // from the bl's own PC. The bl's PC is somewhere
                    // in [lift_addr, lift_addr + fn_size). We don't
                    // have fn_size here but functions are < 16 MiB —
                    // so target - 0x4000000 should be near lift_addr.
                    let delta = a.wrapping_sub(0x0400_0000);
                    delta >= lift_addr && delta < lift_addr.saturating_add(0x0100_0000)
                });
                if is_recursive_marker {
                    branch_stubs.push_str(&format!(
                        "; recursive-bl forwarder for {sym} (rewriter sentinel +0x4000000)\n\
                         define linkonce_odr ptr @{sym}(ptr %state, i64 %pc, ptr %memory) alwaysinline {{\n  \
                           %r_{n} = tail call ptr @sub_{lift_hex}(ptr %state, i64 %pc, ptr %memory)\n  \
                           ret ptr %r_{n}\n\
                         }}\n",
                        sym = ext.sym_name,
                        n = n_suffix,
                        lift_hex = format!("{:x}", lift_addr),
                    ));
                    eprintln!(
                        "[azul-web]   recursive-bl forwarder: {} → sub_{:x}",
                        ext.sym_name, lift_addr,
                    );
                } else {
                    eprintln!(
                        "[azul-web]   unclassified extern: {} — emitting env import",
                        ext.sym_name
                    );
                }
            }
        }
    }
    // attributes #1 marks the JS import. Two key parts:
    //   1. wasm-import-module/wasm-import-name — keeps the declare
    //      as a wasm import (resolved by JS at instantiate-time).
    //   2. memory(readwrite, inaccessiblemem: readwrite) — explicit
    //      side-effect annotation. WITHOUT this, modern LLVM (21+)
    //      treats an attribute-less external import as `memory(none)`
    //      (pure), which lets opt DCE the call along with everything
    //      that depends on it. We were losing the entire dispatch
    //      chain to this in M8.5a until we added the memory clause.
    //
    // The #1 group number is local to this module; both `llvm-link`
    // (subprocess) and `llvm::Linker::linkInModule` (native compile
    // path) auto-renumber attribute groups when merging modules so
    // local-#1 collisions across helper + patched IR can't occur at
    // link time.
    let import_attrs = "attributes #1 = { nounwind \
        memory(readwrite, inaccessiblemem: readwrite) \
        \"wasm-import-module\"=\"env\" \
        \"wasm-import-name\"=\"__az_resolve_callback\" }\n";
    // Shared heap pointer for the bump allocator. linkonce_odr means
    // wasm-ld dedupes across objects → all AzStartup_*'s + any
    // per-callback module that gets the same helper IR see one heap.
    //
    // @__az_call_observer is an anti-DCE sink: every external-call
    // bridge body (AzResolveCallback, AzCallIndirect) stores its
    // result here via `store volatile` so opt can't eliminate the
    // call when the result only flows through the wrapper's local
    // State alloca. The store is observable (volatile + global) so
    // opt must preserve the chain ending in it.
    //
    // The `declare @__az_resolve_callback` is emitted once here so
    // multiple AzResolveCallback bridges in the same helper IR
    // don't duplicate it (which causes wasm-ld to disambiguate
    // them as `__az_resolve_callback.1` and break the JS import).
    // Initial bump pointer: 1 MiB. wasm-ld places the C stack between
    // the data section and ~65 KiB (default 64 KiB stack), so our
    // heap MUST start above the stack-top to avoid overlap.
    // 1 MiB = 1048576 is comfortably past any reasonable stack growth
    // for the eventloop's 5 KiB per-dispatch stack frames. The wasm
    // module declares initial memory ≥16 pages (1 MiB) via the
    // `--initial-memory=1048576` flag in link_objects_to_wasm so the
    // first AzStartup_alloc call is in-bounds without a manual grow.
    // Bump base: 96 MiB. With the synth-addr scheme, every image's
    // text+data sits below this in its assigned band (see
    // `SymbolTable::assign_synthetic_addresses` + the comment in
    // `link_objects_to_wasm`). The heap [96..128 MiB] absorbs
    // ~32 MiB of bump-allocated short-lived data per request;
    // memory.grow extends it at runtime if needed.
    let bump_global = "@__az_bump_ptr = linkonce_odr global i32 100663296, align 4\n\
        @__az_call_observer = linkonce_odr global i32 0, align 4\n\
        declare i32 @__az_resolve_callback(i64) #1\n";
    let _ = export_as; // used inside the format string via the named arg below
    format!(
        r#"; M6 helper module — see `dll/src/web/transpiler_remill.rs::emit_helper_ir`.
target datalayout = "e-m:e-i8:8:32-i16:16:32-i64:64-i128:128-n32:64-S128"
target triple = "aarch64-apple-macosx-macho"

%struct.State = type opaque

define linkonce_odr ptr @__remill_function_return(ptr %state, i64 %pc, ptr %memory) alwaysinline {{
  ret ptr %memory
}}
define linkonce_odr ptr @__remill_function_call(ptr %state, i64 %pc, ptr %memory) alwaysinline {{
  ret ptr %memory
}}
define linkonce_odr ptr @__remill_jump(ptr %state, i64 %pc, ptr %memory) alwaysinline {{
  ret ptr %memory
}}
define linkonce_odr ptr @__remill_missing_block(ptr %state, i64 %pc, ptr %memory) alwaysinline {{
  ; NOTE: must RETURN (not trap) — the cascade/hydration path has hot missing_blocks
  ; (unresolved computed branches) that return-and-continue; trapping here breaks the
  ; cascade (hit-test + layout-real both trapped in AzStartup_hydrateStyledDom).
  ret ptr %memory
}}
define linkonce_odr ptr @__remill_error(ptr %state, i64 %pc, ptr %memory) alwaysinline {{
  ; NOTE: returns (not traps). A hot __remill_error here silently corrupts the lifted fn's
  ; return value (Result→Err, rc=0). The layout hits one (the cascade does not — baselines
  ; stay green when this traps), but PC-capture of %pc proved unreliable for pinpointing
  ; (it lands on clean-lifting fns). See memory m12_cascade_neon_blocker.md.
  ret ptr %memory
}}
; M10-B1.a alias-scope metadata for guest memory ops.
;
; Every `__remill_*memory_*` load/store goes through `inttoptr i64
; %addr to ptr`, which defeats LLVM's standard alias analysis — the
; resulting pointer's provenance is unknown, so AA conservatively
; assumes it might alias the wrapper's State alloca. That defeats
; SROA on the State alloca and keeps a 1088-byte stack buffer alive
; per call (plus all the GEPs into it).
;
; Tagging the inttoptr loads/stores with `!alias.scope !az_guest_list`
; + `!noalias !az_host_list`, combined with a parallel pass on the
; lifted IR that tags every State / local-alloca access with the
; mirror metadata, lets AA prove guest ≠ host. SROA promotes the
; State alloca to register-resident scalars.
;
; Scope identity across helper + lifted IR uses STRING-NAMED scopes
; — llvm-link merges metadata nodes by content, so two structurally
; identical scope/domain nodes from helper and lifted IR collapse to
; one set after link. See `tag_state_accesses` for the mirror side.
define linkonce_odr i8 @__remill_read_memory_8(ptr %memory, i64 %addr) alwaysinline {{
  %p = inttoptr i64 %addr to ptr
  %v = load i8, ptr %p, align 1, !alias.scope !{az_guest_list}, !noalias !{az_host_list}
  ret i8 %v
}}
define linkonce_odr i16 @__remill_read_memory_16(ptr %memory, i64 %addr) alwaysinline {{
  %p = inttoptr i64 %addr to ptr
  %v = load i16, ptr %p, align 2, !alias.scope !{az_guest_list}, !noalias !{az_host_list}
  ret i16 %v
}}
define linkonce_odr i32 @__remill_read_memory_32(ptr %memory, i64 %addr) alwaysinline {{
  %p = inttoptr i64 %addr to ptr
  %v = load i32, ptr %p, align 4, !alias.scope !{az_guest_list}, !noalias !{az_host_list}
  ret i32 %v
}}
define linkonce_odr i64 @__remill_read_memory_64(ptr %memory, i64 %addr) alwaysinline {{
  %p = inttoptr i64 %addr to ptr
  %v = load i64, ptr %p, align 8, !alias.scope !{az_guest_list}, !noalias !{az_host_list}
  ret i64 %v
}}
; M12.7: FP loads (`ldr s/d/q`) lift to __remill_read_memory_f32/f64. Without
; these definitions they're unresolved imports stubbed to 0 at runtime — which
; silently corrupts any FP-register-loaded INTEGER data. hashbrown's NEON
; control-group scan (RawIterRange / match) loads the 8-byte control group via
; `ldr d` → reads 0 → every byte looks FULL (top-bit 0) → the iterator never
; terminates (the layout solver's infinite loop). The value only round-trips
; through memory to a SIMD reg (no FP arithmetic), so a plain typed load
; preserves the exact bits — no NaN canonicalization.
define linkonce_odr float @__remill_read_memory_f32(ptr %memory, i64 %addr) alwaysinline {{
  %p = inttoptr i64 %addr to ptr
  %v = load float, ptr %p, align 4, !alias.scope !{az_guest_list}, !noalias !{az_host_list}
  ret float %v
}}
define linkonce_odr double @__remill_read_memory_f64(ptr %memory, i64 %addr) alwaysinline {{
  %p = inttoptr i64 %addr to ptr
  %v = load double, ptr %p, align 8, !alias.scope !{az_guest_list}, !noalias !{az_host_list}
  ret double %v
}}
define linkonce_odr ptr @__remill_write_memory_f32(ptr %memory, i64 %addr, float %val) alwaysinline {{
  %p = inttoptr i64 %addr to ptr
  store volatile float %val, ptr %p, align 4, !alias.scope !{az_guest_list}, !noalias !{az_host_list}
  ret ptr %memory
}}
define linkonce_odr ptr @__remill_write_memory_f64(ptr %memory, i64 %addr, double %val) alwaysinline {{
  %p = inttoptr i64 %addr to ptr
  store volatile double %val, ptr %p, align 8, !alias.scope !{az_guest_list}, !noalias !{az_host_list}
  ret ptr %memory
}}
define linkonce_odr ptr @__remill_write_memory_8(ptr %memory, i64 %addr, i8 %val) alwaysinline {{
  %p = inttoptr i64 %addr to ptr
  store volatile i8 %val, ptr %p, align 1, !alias.scope !{az_guest_list}, !noalias !{az_host_list}
  ret ptr %memory
}}
define linkonce_odr ptr @__remill_write_memory_16(ptr %memory, i64 %addr, i16 %val) alwaysinline {{
  %p = inttoptr i64 %addr to ptr
  store volatile i16 %val, ptr %p, align 2, !alias.scope !{az_guest_list}, !noalias !{az_host_list}
  ret ptr %memory
}}
define linkonce_odr ptr @__remill_write_memory_32(ptr %memory, i64 %addr, i32 %val) alwaysinline {{
  %p = inttoptr i64 %addr to ptr
  store volatile i32 %val, ptr %p, align 4, !alias.scope !{az_guest_list}, !noalias !{az_host_list}
  ret ptr %memory
}}
define linkonce_odr ptr @__remill_write_memory_64(ptr %memory, i64 %addr, i64 %val) alwaysinline {{
  %p = inttoptr i64 %addr to ptr
  store volatile i64 %val, ptr %p, align 8, !alias.scope !{az_guest_list}, !noalias !{az_host_list}
  ret ptr %memory
}}
define linkonce_odr ptr @__remill_barrier_load_load(ptr %memory) alwaysinline {{ ret ptr %memory }}
define linkonce_odr ptr @__remill_barrier_load_store(ptr %memory) alwaysinline {{ ret ptr %memory }}
define linkonce_odr ptr @__remill_barrier_store_load(ptr %memory) alwaysinline {{ ret ptr %memory }}
define linkonce_odr ptr @__remill_barrier_store_store(ptr %memory) alwaysinline {{ ret ptr %memory }}
; sync_hyper_call (svc/hvc/smc style instructions). Noop pass-through.
define linkonce_odr ptr @__remill_sync_hyper_call(ptr %state, ptr %memory, i32 %call_num) alwaysinline {{ ret ptr %memory }}

; Flag-computation intrinsics. remill emits these to compute the
; result of arithmetic flag effects (zero/sign/carry/overflow); the
; first arg is the result the lift's high-level operator already
; computed. Noop body: return %r unchanged. Variadic-correct: the
; remaining args (the inputs that produced %r) are ignored at this
; level because the lift's high-level arithmetic was lowered with
; native i32/i64 ops that already set the right value in %r.
define linkonce_odr i1 @__remill_flag_computation_sign(i1 %r, ...) alwaysinline {{ ret i1 %r }}
define linkonce_odr i1 @__remill_flag_computation_zero(i1 %r, ...) alwaysinline {{ ret i1 %r }}
define linkonce_odr i1 @__remill_flag_computation_carry(i1 %r, ...) alwaysinline {{ ret i1 %r }}
define linkonce_odr i1 @__remill_flag_computation_overflow(i1 %r, ...) alwaysinline {{ ret i1 %r }}

; Compare-predicate intrinsics. The single arg IS the predicate
; result; the intrinsic is structurally a no-op identity. Same
; rationale as the flag-computation ones.
define linkonce_odr i1 @__remill_compare_eq(i1 %r) alwaysinline {{ ret i1 %r }}
define linkonce_odr i1 @__remill_compare_neq(i1 %r) alwaysinline {{ ret i1 %r }}
define linkonce_odr i1 @__remill_compare_slt(i1 %r) alwaysinline {{ ret i1 %r }}
define linkonce_odr i1 @__remill_compare_sle(i1 %r) alwaysinline {{ ret i1 %r }}
define linkonce_odr i1 @__remill_compare_sgt(i1 %r) alwaysinline {{ ret i1 %r }}
define linkonce_odr i1 @__remill_compare_sge(i1 %r) alwaysinline {{ ret i1 %r }}
define linkonce_odr i1 @__remill_compare_ult(i1 %r) alwaysinline {{ ret i1 %r }}
define linkonce_odr i1 @__remill_compare_ule(i1 %r) alwaysinline {{ ret i1 %r }}
define linkonce_odr i1 @__remill_compare_ugt(i1 %r) alwaysinline {{ ret i1 %r }}
define linkonce_odr i1 @__remill_compare_uge(i1 %r) alwaysinline {{ ret i1 %r }}

; Bump-allocator shared heap pointer (M8.4c). linkonce_odr so
; wasm-ld dedupes across all azul-mini objects + any per-callback
; module emitting the same helper. Initial offset 65536 (64 KiB)
; leaves the wasm stack guard zone alone.
{bump_global}

; JS-imported symbols: attributes block (M8.5a). Marks
; @__az_resolve_callback as a wasm import from `env`.
{import_attrs}
; M7 branch-destination bodies — see `parse_extern_sub_declares` +
; `branch_target_to_host_addr`. Each `sub_<hex>` corresponds to a
; `bl` instruction in the lifted body whose target falls outside
; the byte map remill saw. dladdr-resolved symbols matching
; `__rust_alloc` get a bump-allocator body (M8.4c); everything else
; gets a noop body (M7 / M8.9 will replace specific framework calls
; with imports from azul-mini).
{branch_stubs}

declare ptr @sub_{lift_addr_hex}(ptr noalias, i64, ptr noalias)
declare void @llvm.memset.p0.i64(ptr nocapture writeonly, i8, i64, i1 immarg)
declare void @llvm.memcpy.p0.p0.i64(ptr nocapture writeonly, ptr nocapture readonly, i64, i1 immarg)
declare void @llvm.memmove.p0.p0.i64(ptr nocapture writeonly, ptr nocapture readonly, i64, i1 immarg)

; Callback kind: {kind}. Wrapper synthesized from
; the matching `CallbackSignature`. PCS table at the top of
; `transpiler_remill.rs`. Exports as `{export_as}` so wasm-ld can
; surface it to the loader / JS.
define {ret_ty} @{export_as}({params}) {{
  ; M12.8: stack_buf MUST come first (higher addr) so that lifted
  ; AArch64 `stp/str ..., [sp, #+N]` writes (positive offsets)
  ; land in the caller's wasm-stack space — not in %state_buf.
  ; Wasm-stack grows down, so the SECOND alloca is at a LOWER
  ; address. If state_buf were first, writes at SP+N (N<1088)
  ; would land in state_buf and corrupt State.SP at offset 1040,
  ; triggering downstream traps when later code re-reads SP.
  ;
  ; Stack scratch: SP-relative spills land here. Its address IS
  ; ptrtoint'd (for the initial SP value), so SROA can't promote
  ; this one — but it's small and self-contained.
  %stack_buf = alloca [{stack_size} x i8], align 16
  ; State: register-file storage. Strictly aliased (no `ptrtoint`
  ; ever taken of `%state_buf`), so opt -O2's SROA can promote it
  ; into individual scalar slots after the lifted body inlines.
  %state_buf = alloca [{state_size} x i8], align 16

  call void @llvm.memset.p0.i64(ptr %state_buf, i8 0, i64 {state_size}, i1 false)

{prologue}
  ; SP register holds the address of the top of %stack_buf as an
  ; i64. The lifted body decrements toward lower addresses within
  ; the stack buffer; loads/stores via inttoptr land in-bounds.
  ; This is the ONLY ptrtoint in the wrapper — and it's of
  ; %stack_buf, not %state_buf, so %state_buf stays SROA-eligible.
  %sp_top = getelementptr inbounds i8, ptr %stack_buf, i64 {stack_size}
  %sp_int = ptrtoint ptr %sp_top to i64
  %sp_slot = getelementptr inbounds i8, ptr %state_buf, i64 {sp_off}
  store i64 %sp_int, ptr %sp_slot, align 8

  ; Memory token is null — every memory op was lowered to a real
  ; load/store above, so the token is dead inside the body.
  %_ret_mem = call ptr @sub_{lift_addr_hex}(ptr %state_buf, i64 {lift_addr_dec}, ptr null)

{ret_code}}}

; M10-B1.a alias-scope graph. Two disjoint scopes in one domain:
; "guest" wraps inttoptr-loaded linear-memory pointers,
; "host" wraps the wrapper's State alloca + lifted-IR local
; allocas. Cross-module identity by content: llvm-link uniques
; metadata nodes by structural hash, so the equivalent definitions
; emitted by `tag_state_accesses` in the lifted IR merge into the
; same scope nodes after link.
!{az_domain_id} = !{{!"az_alias_domain"}}
!{az_guest_scope_id} = !{{!"az_guest_scope", !{az_domain_id}}}
!{az_host_scope_id} = !{{!"az_host_scope", !{az_domain_id}}}
!{az_guest_list} = !{{!{az_guest_scope_id}}}
!{az_host_list} = !{{!{az_host_scope_id}}}
"#,
        kind = sig.kind,
        ret_ty = ret_ty,
        params = params,
        prologue = prologue.trim_end(),
        ret_code = ret_code,
        lift_addr_hex = format!("{:x}", lift_addr),
        lift_addr_dec = lift_addr,
        sp_off = sp_off,
        state_size = state_size,
        stack_size = stack_size,
        branch_stubs = branch_stubs.trim_end(),
        bump_global = bump_global.trim_end(),
        import_attrs = import_attrs.trim_end(),
        export_as = export_as,
        az_domain_id = AZ_DOMAIN_MD_ID,
        az_guest_scope_id = AZ_GUEST_SCOPE_MD_ID,
        az_host_scope_id = AZ_HOST_SCOPE_MD_ID,
        az_guest_list = AZ_GUEST_LIST_MD_ID,
        az_host_list = AZ_HOST_LIST_MD_ID,
    )
}

/// Parse the lifted IR for `declare ptr @sub_<hex>(...)` entries
/// (excluding the lift entry `sub_<lift_addr_hex>` which has a
/// `define`, not a `declare`). Returns the bare symbol names like
/// `"sub_fffffda4"`.
///
/// **M8.8 Stage 1**: post-rewrite IR no longer carries `.N` suffix
/// duplicates — `rewrite_sub_names_to_canonical` collapsed every
/// reference to the same canonical address into one name. So this
/// parser doesn't need the per-call-site dedup logic anymore.
fn parse_extern_sub_declares(ir: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for line in ir.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("declare ptr @sub_") else {
            continue;
        };
        let Some(paren_idx) = rest.find('(') else {
            continue;
        };
        let hex_part = &rest[..paren_idx];
        if !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        let sym = format!("sub_{}", hex_part);
        if seen.insert(sym.clone()) {
            out.push(sym);
        }
    }
    out
}

/// Collapse repeated `declare ptr @sub_<hex>(...)` lines AND drop
/// declares for symbols that already have a `define` in the same
/// module. Either case is `error: invalid redefinition` from
/// llvm-link / opt.
///
/// Two situations trigger this:
///
///   1. After `rewrite_sub_names_to_canonical`, two `.N`-suffixed
///      externs that mapped to the same canonical address both
///      became `declare ptr @sub_<canonical>(...)`.
///
///   2. The IR's entry function self-recurses (e.g. AzStartup_alloc
///      calling itself). remill emits `declare ptr @sub_<entry>` for
///      the call site + `define ptr @sub_<entry>.2` for the body
///      definition. After the rewriter strips `.2`, both end up
///      named `sub_<entry>` and LLVM rejects the double-declaration.
///
/// We do a two-pass: first scan for every `define ... @sub_<hex>(`
/// to learn which symbols are defined, then drop redundant
/// `declare`s. The pass is scoped to `sub_<hex>` lines so other
/// LLVM IR (`define linkonce_odr ptr @__remill_*` etc.) isn't
/// touched.
fn dedup_sub_declares(ir: &str) -> String {
    use std::collections::HashSet;
    // Pass 1: enumerate defined sub_<hex> names.
    let mut defined: HashSet<String> = HashSet::new();
    for line in ir.lines() {
        let trimmed = line.trim_start();
        // "define ... @sub_<hex>("
        if let Some(idx) = trimmed.find("@sub_") {
            let after_at = &trimmed[idx + 1..];
            if let Some(paren) = after_at.find('(') {
                let name = &after_at[..paren];
                if let Some(hex) = name.strip_prefix("sub_") {
                    if hex.chars().all(|c| c.is_ascii_hexdigit())
                        && trimmed.starts_with("define ")
                    {
                        defined.insert(name.to_string());
                    }
                }
            }
        }
    }
    // Pass 2: emit lines, skipping declares whose name is already
    // defined OR whose declare we've already emitted.
    let mut out = String::with_capacity(ir.len());
    let mut seen_declares: HashSet<String> = HashSet::new();
    for line in ir.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("declare ptr @sub_") {
            if let Some(paren_idx) = rest.find('(') {
                let hex_part = &rest[..paren_idx];
                if hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
                    let name = format!("sub_{}", hex_part);
                    if defined.contains(&name) {
                        // A `define` exists for this name; the
                        // declare is redundant.
                        continue;
                    }
                    if !seen_declares.insert(hex_part.to_string()) {
                        // Already emitted this declare.
                        continue;
                    }
                }
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// M10-B1.a — post-process the lifted IR (or wrapper helper IR) to
/// tag every `load` / `store` with `!alias.scope !host_list` +
/// `!noalias !guest_list` metadata. Combined with the helper IR's
/// guest-tagged `__remill_*memory_*` bodies, this lets LLVM's scoped
/// alias analysis prove that the wrapper's State alloca (host) and
/// the lifted body's inttoptr loads (guest) don't alias — SROA then
/// promotes State to scalar registers, layout.wasm shrinks
/// significantly.
///
/// The metadata definitions are appended at the bottom of every IR
/// the lift pipeline produces. llvm-link uniques metadata nodes by
/// structural content, so the identical `!{!"az_alias_domain"}`,
/// `!{!"az_guest_scope", !{!"az_alias_domain"}}`, etc. emitted by
/// every module collapse to one set of nodes after link.
///
/// Skips lines that already carry `!alias.scope` / `!noalias`
/// metadata (the helper IR's memory intrinsics are tagged at
/// emission with the guest scope and must keep that classification).
///
/// Atomic loads/stores (`load atomic`, `store atomic`) and volatile
/// variants are detected via the keyword and treated identically.
/// Lines whose tail is a `; preds = …` comment get metadata inserted
/// BEFORE the comment.
/// M12.5d — replace AArch64 target triple / datalayout in remill's
/// lifted IR with wasm32 equivalents. Without this, opt + InstCombine
/// + SROA run with i64 pointers (per AArch64 datalayout `n32:64`),
/// then llc retargets at emission. The mismatch causes pointer
/// arithmetic on the State struct to use i64 ops, and SROA splits
/// State fields with i64-aligned slots. The wasm output then has
/// function signatures like `(i64, i64, i32) -> i32` for state-ptr
/// args, and cross-function state propagation breaks because callees
/// see the state ptr's low-32 truncated address but write/read at
/// i64-offset boundaries.
///
/// Rewriting the header to wasm32-unknown-unknown lets opt run with
/// 32-bit pointers from the start, so SROA splits State on i32
/// boundaries and the wasm output uses `(i32, i64, i32) -> i32`
/// signatures consistently.
fn retarget_to_wasm32(ir: &str) -> String {
    let mut out = String::with_capacity(ir.len());
    let mut saw_datalayout = false;
    let mut saw_triple = false;
    for line in ir.lines() {
        let trimmed = line.trim_start();
        if !saw_datalayout && trimmed.starts_with("target datalayout = ") {
            out.push_str(
                "target datalayout = \"e-m:e-p:32:32-p10:8:8-p20:8:8-i64:64-n32:64-S128-ni:1:10:20\"\n",
            );
            saw_datalayout = true;
            continue;
        }
        if !saw_triple && trimmed.starts_with("target triple = ") {
            out.push_str("target triple = \"wasm32-unknown-unknown\"\n");
            saw_triple = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// M12.5d-fix — strip `noalias` attribute from lifted sub_* function
/// args. Remill emits `define ptr @sub_<addr>(ptr noalias %state,
/// i64 %pc, ptr noalias %memory)`. The `noalias` on %state and
/// %memory enables LLVM's cross-function alias analysis to make
/// aggressive assumptions about which addresses different
/// functions can access. With many lifted functions in one
/// translation unit, this causes optimization passes to
/// incorrectly conclude that other functions' const-pool reads
/// don't alias the current function's accesses, leading to
/// dropped/reordered loads in the wasm output.
///
/// Bisect verified (commit 62a4ada71): adding a single function
/// with `(&mut T) -> Struct` signature corrupts UNRELATED lifted
/// functions' const-pool reads. By-value args don't trigger it.
/// The Rust `&mut` lowers to LLVM `ptr noalias`.
///
/// This pass removes `noalias` from sub_* function definitions
/// and declarations so the optimizer treats lifted args
/// conservatively (may-alias). Performance cost is minimal —
/// lifted bodies don't aggressively share memory across calls
/// anyway.
fn strip_noalias_from_sub_args(ir: &str) -> String {
    let mut out = String::with_capacity(ir.len());
    for line in ir.lines() {
        let trimmed = line.trim_start();
        // Match define/declare lines for sub_<hex> functions and
        // remove `noalias` from their arg types.
        if (trimmed.starts_with("define ") || trimmed.starts_with("declare "))
            && trimmed.contains("@sub_")
        {
            // Strip ` noalias` (with leading space) — only in arg
            // type lists (parens). Simple text replacement is safe
            // because `noalias` only appears in attribute positions.
            let stripped = line.replace(" noalias", "");
            out.push_str(&stripped);
            out.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn tag_state_accesses(ir: &str) -> String {
    let mut out = String::with_capacity(ir.len() + 4 * 1024);
    let host_tail = format!(
        ", !alias.scope !{}, !noalias !{}",
        AZ_HOST_LIST_MD_ID, AZ_GUEST_LIST_MD_ID,
    );

    for line in ir.lines() {
        let trimmed = line.trim_start();
        let is_load = trimmed.starts_with("load ")
            || (trimmed.starts_with('%')
                && trimmed.contains(" = load ")
                && !trimmed.contains(" = load atomic ")  // load atomic handled below by keyword
                || trimmed.contains(" = load atomic "));
        let is_store = trimmed.starts_with("store ");

        if !(is_load || is_store) {
            out.push_str(line);
            out.push('\n');
            continue;
        }

        // Lift-emitted `call ptr @__remill_*memory_*(...)` lines start
        // with `%X = call ...` and aren't load/store — already filtered
        // above.

        // Skip if metadata is already present (helper IR's memory ops
        // are pre-tagged with the guest scope).
        if line.contains("!alias.scope") || line.contains("!noalias") {
            out.push_str(line);
            out.push('\n');
            continue;
        }

        // Locate the position to insert metadata. If a trailing
        // `; preds = ...` (or any other comment) exists, insert
        // BEFORE the semicolon; otherwise append to end of line.
        let comment_idx = line.find(';');
        if let Some(idx) = comment_idx {
            let before = line[..idx].trim_end();
            let after = &line[idx..];
            out.push_str(before);
            out.push_str(&host_tail);
            out.push_str("  ");
            out.push_str(after);
        } else {
            out.push_str(line.trim_end());
            out.push_str(&host_tail);
        }
        out.push('\n');
    }

    // Append metadata definitions. Use string-named nodes so
    // llvm-link merges identical definitions from helper / patched /
    // lifted IRs into one scope graph.
    //
    // Skip appending if the IR already contains the domain def — the
    // helper IR template emits its own copy at format time, and we're
    // also invoked on the post-format helper IR (to tag the wrapper
    // prologue/return + bump-alloc bodies' host accesses). Without
    // this guard the same `!{az_domain_id}` would be defined twice,
    // and LLVM's parser errors with "Metadata id is already used".
    let domain_def_marker = format!("!{} = !", AZ_DOMAIN_MD_ID);
    if !out.contains(&domain_def_marker) {
        out.push_str(&format!(
            "\n; M10-B1.a alias-scope metadata (mirror of helper IR).\n\
             !{az_domain_id} = !{{!\"az_alias_domain\"}}\n\
             !{az_guest_scope_id} = !{{!\"az_guest_scope\", !{az_domain_id}}}\n\
             !{az_host_scope_id} = !{{!\"az_host_scope\", !{az_domain_id}}}\n\
             !{az_guest_list} = !{{!{az_guest_scope_id}}}\n\
             !{az_host_list} = !{{!{az_host_scope_id}}}\n",
            az_domain_id = AZ_DOMAIN_MD_ID,
            az_guest_scope_id = AZ_GUEST_SCOPE_MD_ID,
            az_host_scope_id = AZ_HOST_SCOPE_MD_ID,
            az_guest_list = AZ_GUEST_LIST_MD_ID,
            az_host_list = AZ_HOST_LIST_MD_ID,
        ));
    }

    out
}

/// Scan a function's raw .text bytes for ARM64 `BL imm26` and
/// `B imm26` instructions and return the absolute target addresses.
/// Used by [`pre_walk_transitive_deps`] to discover dependencies via
/// byte-pattern inspection BEFORE lifting — enables batched lift to
/// cover the entire dep set in one call.
///
/// Both BL (call) and B (branch / tail-call) use the same imm26
/// encoding. B targets that fall inside the function's own byte
/// range are intra-function branches (loops, if/else); the BFS
/// caller filters them via visited-set tracking. B targets outside
/// the range are tail-call shims which the SymbolTable's
/// `detect_arm64_tail_shims` would normally collapse, so they hit
/// the same canonical address as the chained target — visited-set
/// dedup at the caller side still handles it.
///
/// BLR / BR (indirect call / jump) cannot be statically resolved
/// from bytes — they route through __az_call_indirect machinery
/// and are bridged in helper IR, not via direct dep traversal.
/// M9-after-review: scan ARM64 bytes for `adrp` instructions and
/// return their target PAGE addresses (page-aligned native addrs).
/// Used by `link_objects_to_wasm` to compute the precise set of
/// 4 KiB pages each wasm needs mirrored, instead of indiscriminately
/// shipping every byte of `libazul`'s `__const` / `__cstring` (~27 MiB
/// for the lifter-static build, mostly LLVM string tables that the
/// user's cb never touches).
///
/// ARM64 `adrp x<n>, imm21`:
///   - opcode bits 31, 28..24 = `1xx10000` (top byte is `0x90` or `0x91`)
///   - bit 31 selects ADR (0) vs ADRP (1) — we only care about ADRP
///   - immediate is 21 bits split as `immlo[1:0]` (bits 30..29) +
///     `immhi[20:2]` (bits 23..5), shifted left by 12 → page offset
///
/// Target page = `(pc & ~0xFFF) + sign_extend(imm21, 33) << 12`.
fn scan_arm64_adrp_pages(fn_bytes: &[u8], fn_addr: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut offset = 0;
    while offset + 4 <= fn_bytes.len() {
        let instr = u32::from_le_bytes([
            fn_bytes[offset],
            fn_bytes[offset + 1],
            fn_bytes[offset + 2],
            fn_bytes[offset + 3],
        ]);
        let pc = fn_addr.wrapping_add(offset);
        // ADR / ADRP family: bits 28..24 = 10000. Bit 31 = 0 → ADR
        // (byte-offset, ±1 MiB), 1 → ADRP (page-offset, ±4 GiB shifted).
        if ((instr >> 24) & 0x1F) == 0x10 {
            let immlo = (instr >> 29) & 0x3;
            let immhi = (instr >> 5) & 0x7FFFF;
            let imm21 = (immhi << 2) | immlo;
            let signed_imm: i64 = if imm21 & (1 << 20) != 0 {
                (imm21 as i64) | !0x1F_FFFF_i64
            } else {
                imm21 as i64
            };
            let target = if (instr >> 31) == 1 {
                // ADRP: shift << 12, target is page-aligned
                let pc_page = pc & !0xFFF;
                (pc_page as i64).wrapping_add(signed_imm << 12) as usize
            } else {
                // ADR: byte offset from current PC
                (pc as i64).wrapping_add(signed_imm) as usize
            };
            // Round to page boundary for the mirror.
            out.push(target & !0xFFF);
        }
        // LDR (literal): pc-relative load with embedded immediate.
        // Encoding bits 31..30 + 29..27 = `00/01_011_<v=0>_00`
        // For 32-bit/64-bit non-vector LDR-literal: top 8 bits == 0x18 (i32)
        // or 0x58 (i64). immediate19 at bits 23..5, shifted << 2.
        else if (instr >> 24) == 0x18 || (instr >> 24) == 0x58 {
            let imm19 = (instr >> 5) & 0x7FFFF;
            let signed_imm: i64 = if imm19 & (1 << 18) != 0 {
                ((imm19 as i64) | !0x7FFFF_i64) << 2
            } else {
                (imm19 as i64) << 2
            };
            let target = (pc as i64).wrapping_add(signed_imm) as usize;
            out.push(target & !0xFFF);
        }
        offset += 4;
    }
    out
}

/// M10-E1 — exact-range scan for ADRP-anchored memory accesses.
///
/// Returns a list of `(native_addr, len)` pairs covering the exact
/// byte ranges the function will load from per-page data sections.
/// Recognizes three idioms:
///
/// 1. `adrp Xn, page` + `ldr Xt, [Xn, #lo12]` (single load).
/// 2. `adrp Xn, page` + `add Xn, Xn, #lo12` + chain of `ldr` /
///    `str` / `add` (computed-pointer base; we conservatively bound
///    the range as `[page+lo12, page+lo12+128]` — covers a small
///    struct or pointer table).
/// 3. `adrp Xn, page` + `ldr Wt, [Xn, #lo12]` (32-bit load).
///
/// Patterns that don't fit fall through to the legacy whole-page
/// mirror via [`scan_arm64_adrp_pages`]. The caller can dedup the
/// two sets — exact ranges take precedence; if an exact range is
/// found for a page, the whole-page mirror is skipped.
fn scan_arm64_adrp_accesses(
    fn_bytes: &[u8],
    fn_addr: usize,
) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut adrp_targets: [Option<usize>; 32] = [None; 32];
    let mut offset = 0;
    while offset + 4 <= fn_bytes.len() {
        let instr = u32::from_le_bytes([
            fn_bytes[offset],
            fn_bytes[offset + 1],
            fn_bytes[offset + 2],
            fn_bytes[offset + 3],
        ]);
        let pc = fn_addr.wrapping_add(offset);

        // ADR / ADRP encoding (bits 28..24 = 10000). Bit 31 = 0 →
        // ADR (byte-aligned target, exact address), 1 → ADRP
        // (page-aligned target, page-truncated address).
        //
        // M10-E3: ADR is used for LLVM jump-table dispatch:
        //   ADR  Xj, <table>
        //   LDRB Wk, [Xj, Wm, UXTW]
        //   ADD  Xj, Xj, Wk, LSL #2
        //   BR   Xj
        // The table sits inline in __TEXT immediately after the
        // dispatching block. Emit a 256-byte conservative range so
        // the mirror grabs the table bytes (up to 256 switch cases).
        if ((instr >> 24) & 0x1F) == 0x10 {
            let immlo = (instr >> 29) & 0x3;
            let immhi = (instr >> 5) & 0x7FFFF;
            let imm21 = (immhi << 2) | immlo;
            let signed_imm: i64 = if imm21 & (1 << 20) != 0 {
                (imm21 as i64) | !0x1F_FFFF_i64
            } else {
                imm21 as i64
            };
            let target = if (instr >> 31) == 1 {
                let pc_page = pc & !0xFFF;
                (pc_page as i64).wrapping_add(signed_imm << 12) as usize
            } else {
                (pc as i64).wrapping_add(signed_imm) as usize
            };
            let rd = (instr & 0x1F) as usize;
            adrp_targets[rd] = Some(target);
            if (instr >> 31) == 0 {
                // ADR: target is exact. Emit a conservative byte range
                // for jump-table content immediately after the ADR.
                out.push((target, 256));
            }
            offset += 4;
            continue;
        }

        // LDRB (register, UXTW extension): top byte 0x38, bits 21=1,
        // bits 15..13 = 010 (UXTW), bit 11 = 1, bit 10 = 0. Mask:
        // (instr & 0xFFE0_0C00) == 0x3860_0800
        //
        // Used after `ADR Xn, table` to load a byte offset from the
        // table indexed by another register. If we know Rn's address,
        // emit a conservative 256-byte range covering typical
        // jump-table sizes (up to 256 cases). Same fix as the ADR
        // arm above, reached when the LDRB is decoded.
        if (instr & 0xFFE0_0C00) == 0x3860_0800 {
            let rn = ((instr >> 5) & 0x1F) as usize;
            if let Some(base) = adrp_targets[rn] {
                out.push((base, 256));
            }
            let rt = (instr & 0x1F) as usize;
            adrp_targets[rt] = None;
            offset += 4;
            continue;
        }

        // ADD (immediate, 64-bit): top 8 bits == 0x91 (with shift=0,
        // S=0). Encoding: sf=1, op=0, S=0, 100010, sh, imm12, Rn, Rd.
        // Use 0xFF800000 mask to match (sf=1, opc=00100010, sh=0/1).
        if (instr & 0xFF80_0000) == 0x9100_0000 {
            let sh = (instr >> 22) & 0x1;
            let imm12 = ((instr >> 10) & 0xFFF) as usize;
            let rn = ((instr >> 5) & 0x1F) as usize;
            let rd = (instr & 0x1F) as usize;
            if let Some(base) = adrp_targets[rn] {
                let lo12 = if sh == 1 { imm12 << 12 } else { imm12 };
                let new_target = base.wrapping_add(lo12);
                // Propagate to Rd.
                adrp_targets[rd] = Some(new_target);
                // Conservative range: 64 bytes covers a typical
                // pointer-table line. Most accesses through this
                // pointer get emitted as their own precise ranges by
                // the LDR/LDP scanners below; this emit only matters
                // when the pointer is passed to another fn (whose
                // scan wouldn't re-derive the page from this adrp).
                out.push((new_target, 64));
            } else {
                adrp_targets[rd] = None;
            }
            offset += 4;
            continue;
        }

        // MOV (register, alias of ORR with zero register): copy
        // adrp_targets[Rm] → adrp_targets[Rd]. Without this we lose
        // the address through a register-shuffle. Encoding:
        // ORR Xd, XZR, Xm → bits = sf||01_01010|0||0|0|Xm|0|0|0|Xn=31|Xd
        // For mov xN, xM: instr = 0xAA0003E0 | (m<<16) | (d<<0).
        // Match top 16 bits 0xAA00 with Xn (bits 9..5) == 0b11111 (31).
        if (instr & 0xFFE0_FC1F) == 0xAA00_03E0 {
            let rm = ((instr >> 16) & 0x1F) as usize;
            let rd = (instr & 0x1F) as usize;
            adrp_targets[rd] = adrp_targets[rm];
            offset += 4;
            continue;
        }

        // LDUR/STUR (immediate, unscaled offset, signed 9-bit).
        //   Top 8 bits: 0xF8 (X), 0xB8 (W), 0x78 (H), 0x38 (B), 0xFC/0xBC (FP).
        //   Distinguished from regular LDR by bits 11..10 = 00 and bit 21 = 0.
        //   imm9 at bits 20..12, sign-extended.
        // M12.5c: LDUR Qt also has top8 = 0x3C but bit 23 = 1 — must
        // be checked BEFORE the byte-width arm below.
        // M12.5d-fix: DO NOT include top8 = 0x3D in the Q-LDUR
        // detection. 0x3D is LDR-scaled (unsigned offset), NOT LDUR.
        // Previously this clause incorrectly matched LDR Q whose
        // imm12 % 4 == 0 (every other one in make_test_struct's
        // const-pool reads), causing the scanner to record bogus
        // 16-byte ranges at imm9 instead of the real imm12*16
        // targets. Result: every other q0 LDR target was missed,
        // leaving the mirror with zero bytes at those addresses
        // and the sret heap with corresponding 16-byte gaps.
        let top8_unscaled = instr >> 24;
        let unscaled_w_for_top8: Option<usize> = if top8_unscaled == 0x3C
            && ((instr >> 23) & 1) == 1
        {
            Some(16)
        } else {
            match top8_unscaled {
                0xF8 | 0xFC => Some(8),
                0xB8 | 0xBC => Some(4),
                0x78 | 0x7C => Some(2),
                0x38 | 0x3C => Some(1),
                _ => None,
            }
        };
        if let Some(w) = unscaled_w_for_top8 {
            if ((instr >> 21) & 0x1) == 0 && ((instr >> 10) & 0x3) == 0 {
                let imm9_raw = ((instr >> 12) & 0x1FF) as i32;
                let imm9: i32 = if imm9_raw & 0x100 != 0 {
                    imm9_raw | !0x1FF
                } else {
                    imm9_raw
                };
                let rn = ((instr >> 5) & 0x1F) as usize;
                let rt = (instr & 0x1F) as usize;
                if let Some(base) = adrp_targets[rn] {
                    let target = (base as isize).wrapping_add(imm9 as isize) as usize;
                    out.push((target, w));
                    adrp_targets[rt] = None;
                }
                offset += 4;
                continue;
            }
        }

        // LDP/STP (GPR and SIMD&FP): family of pair-load/store ops.
        //
        // GPR variants (V=0):
        //   X signed offset:  0xA940_0000 (LDP) / 0xA900_0000 (STP)  width=16 scale=8
        //   X pre-index:      0xA9C0_0000 (LDP) / 0xA8C0_0000 (STP)
        //   X post-index:     0xA980_0000 (LDP) / 0xA880_0000 (STP)
        //   W signed offset:  0x2940_0000 (LDP) / 0x2900_0000 (STP)  width=8  scale=4
        //
        // SIMD&FP variants (V=1) — same layout, bit 26 set:
        //   Q signed offset:  0xAD40_0000 (LDP) / 0xAD00_0000 (STP)  width=32 scale=16
        //   Q pre-index:      0xADC0_0000 (LDP) / 0xAC80_0000 (STP)   <-- per ARM ARM
        //   Q post-index:     0xACC0_0000 (LDP) / 0xAC80_0000 (STP)
        //   D signed offset:  0x6D40_0000 (LDP) / 0x6D00_0000 (STP)  width=16 scale=8
        //   D pre-index:      0x6DC0_0000 (LDP) / 0x6C80_0000 (STP)
        //   D post-index:     0x6CC0_0000 (LDP) / 0x6C80_0000 (STP)
        //   S signed offset:  0x2D40_0000 (LDP) / 0x2D00_0000 (STP)  width=8  scale=4
        //   S pre-index:      0x2DC0_0000 (LDP) / 0x2C80_0000 (STP)
        //   S post-index:     0x2CC0_0000 (LDP) / 0x2C80_0000 (STP)
        //
        // For mirror purposes we only need: (base, byte_imm, total_width).
        // Determine width/scale from the top 2 bits (opc[1:0]) and bit 26 (V).
        let masked = instr & 0xFFC0_0000;
        let is_ldp_stp_family = {
            // top10 must be `XX_0_011_010_X` (signed-offset)
            // or `XX_0_011_011_X` (pre/post-index) for GPR (V=0)
            // or `XX_1_011_010_X` / `XX_1_011_011_X` for SIMD (V=1).
            let top10 = (instr >> 22) & 0x3FF;
            // top10 bits 9..7 = opc[1:0]V, bits 6..1 = 10110 + (signed or indexed bit), bit 0 = L
            // We accept any of:
            //   00 010110 1 X (W signed/STP+LDP)
            //   01 010110 1 X (S signed STP/LDP — V=1)
            //   10 010110 1 X (X signed)
            //   10 010111 1 X (X pre-idx)
            //   10 010110 0 X (X post-idx)   ← (bit 23 differentiates)
            // Simpler: bits 28..25 must be `1011`, bit 27..25 = 011, bit 24 part of index mode.
            //
            // Cleaner check: bits 28..25 = 0b1011 and bit 24 distinguishes indexed (=0) vs
            // signed/no-allocate (=1) — except this isn't quite right either.
            //
            // Easiest: enumerate the 24 valid masks. (Cheap, exhaustive.)
            const LDP_STP_MASKS: &[u32] = &[
                // GPR X
                0xA940_0000, 0xA900_0000, 0xA9C0_0000, 0xA980_0000,
                0xA8C0_0000, 0xA880_0000,
                // GPR W
                0x2940_0000, 0x2900_0000, 0x29C0_0000, 0x2980_0000,
                0x28C0_0000, 0x2880_0000,
                // SIMD Q
                0xAD40_0000, 0xAD00_0000, 0xADC0_0000, 0xAD80_0000,
                0xACC0_0000, 0xAC80_0000,
                // SIMD D
                0x6D40_0000, 0x6D00_0000, 0x6DC0_0000, 0x6D80_0000,
                0x6CC0_0000, 0x6C80_0000,
                // SIMD S
                0x2D40_0000, 0x2D00_0000, 0x2DC0_0000, 0x2D80_0000,
                0x2CC0_0000, 0x2C80_0000,
            ];
            let _ = top10;
            LDP_STP_MASKS.iter().any(|m| masked == *m)
        };
        if is_ldp_stp_family {
            // opc[1:0] at bits 31..30 selects size; V at bit 26 selects GPR vs SIMD.
            let opc = (instr >> 30) & 0x3;
            let v = (instr >> 26) & 0x1;
            let (width, scale): (usize, usize) = match (v, opc) {
                (0, 0) => (8, 4),    // LDP/STP W
                (0, 2) => (16, 8),   // LDP/STP X
                (1, 0) => (8, 4),    // LDP/STP S
                (1, 1) => (16, 8),   // LDP/STP D
                (1, 2) => (32, 16),  // LDP/STP Q
                _ => { offset += 4; continue; }
            };
            // imm7 at bits 21..15, sign-extended.
            let imm7_raw = ((instr >> 15) & 0x7F) as i32;
            let imm7: i32 = if imm7_raw & 0x40 != 0 {
                imm7_raw | !0x7F
            } else {
                imm7_raw
            };
            let byte_imm = (imm7 as isize) * (scale as isize);
            let rn = ((instr >> 5) & 0x1F) as usize;
            if let Some(base) = adrp_targets[rn] {
                let target = (base as isize).wrapping_add(byte_imm) as usize;
                out.push((target, width));
            }
            // LDP loads into two regs — both become "unknown" addresses.
            // For GPR variants this is meaningful; for SIMD it has no
            // effect because adrp_targets indexes GPRs only.
            if v == 0 {
                let rt = (instr & 0x1F) as usize;
                let rt2 = ((instr >> 10) & 0x1F) as usize;
                adrp_targets[rt] = None;
                adrp_targets[rt2] = None;
            }
            offset += 4;
            continue;
        }

        // LDR/STR (immediate, unsigned offset): three groups by top byte.
        //   0xF9 → LDR Xt, [Xn, #imm*8]   width=8
        //   0xB9 → LDR Wt, [Xn, #imm*4]   width=4
        //   0x79 → LDRH Wt, [Xn, #imm*2]  width=2
        //   0x39 → LDRB Wt, [Xn, #imm]    width=1
        //   0xFD → LDR Dt, [Xn, #imm*8]   width=8 (FP)
        //   0xBD → LDR St, [Xn, #imm*4]   width=4 (FP)
        //   0xF8 / 0xB8 → STR/LDR unscaled (LDUR/STUR) — also match.
        // Store variants share encoding except for opc bits — same
        // address calculation so we treat them identically for the
        // mirror (we only READ from the data anyway; the lifted code
        // may STORE back into mirrored locations, which is fine).
        let top8 = instr >> 24;
        let (width, scale): (usize, usize) = match top8 {
            0xF9 | 0xFD => (8, 8),
            0xB9 | 0xBD => (4, 4),
            0x79 | 0x7D => (2, 2),
            // SIMD LDR Qt 128-bit must precede the 0x3D 8-bit case —
            // both ldr Bt and ldr Qt have top8 = 0x3D, distinguished by
            // bit 23 of opc (opc[1] = 1 means 128-bit Q).
            // BUG FIX (M12.5b): previously this arm was unreachable
            // because `0x39 | 0x3D => (1, 1)` matched first, so all
            // ldr q?, [xn, #imm] loads only mirrored 1 byte → const pool
            // reads returned zero → all sret writes stored zeros.
            0x3D if (instr >> 23) & 1 == 1 => (16, 16),
            0x39 | 0x3D => (1, 1),
            _ => {
                offset += 4;
                continue;
            }
        };
        let imm12 = ((instr >> 10) & 0xFFF) as usize;
        let rn = ((instr >> 5) & 0x1F) as usize;
        let rt = (instr & 0x1F) as usize;
        if let Some(base) = adrp_targets[rn] {
            let target = base.wrapping_add(imm12 * scale);
            out.push((target, width));
            // Propagate the loaded value's tracking: we can't know
            // what's at the target without reading memory at compile
            // time; clear Rt as a known-address.
            adrp_targets[rt] = None;
        }
        offset += 4;
    }
    out
}

fn scan_arm64_bl_b_targets(fn_bytes: &[u8], fn_addr: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut offset = 0;
    while offset + 4 <= fn_bytes.len() {
        let instr = u32::from_le_bytes([
            fn_bytes[offset],
            fn_bytes[offset + 1],
            fn_bytes[offset + 2],
            fn_bytes[offset + 3],
        ]);
        let opcode_top6 = instr >> 26;
        // BL = 0b100101 (0x25), B = 0b000101 (0x05). Both use imm26.
        if opcode_top6 == 0x25 || opcode_top6 == 0x05 {
            let imm26 = instr & 0x03FF_FFFF;
            // Sign-extend 26 bits → i32.
            let signed: i32 = if imm26 & (1 << 25) != 0 {
                (imm26 | 0xFC00_0000) as i32
            } else {
                imm26 as i32
            };
            let pc = fn_addr.wrapping_add(offset);
            let target = (pc as isize).wrapping_add((signed as isize) * 4);
            if target >= 0 {
                out.push(target as usize);
            }
        }
        offset += 4;
    }
    out
}

/// Parse a `sub_<hex>` symbol name back into a host address. Returns
/// `None` for malformed inputs. After
/// `rewrite_sub_names_to_canonical`, every `sub_<hex>` reference in
/// the IR uses the canonical address as its hex, so this is a direct
/// host-address parser — no lift-space arithmetic needed.
fn parse_sub_hex_as_addr(sym_name: &str) -> Option<usize> {
    let hex = sym_name.strip_prefix("sub_")?;
    usize::from_str_radix(hex, 16).ok()
}

/// Post-lift IR rewriter — Stage 1's load-bearing change.
///
/// Walks every `@sub_<hex>[.N]` token in the lifted IR and rewrites
/// it to `@sub_<canonical_addr_hex>` based on the SymbolTable's
/// chain map. This single pass dissolves four pre-M8.8 problems:
///
///   1. **`.N` suffix dedup** — remill emits `sub_<addr>.2`,
///      `sub_<addr>.3`, … for repeat call sites of the same `bl`
///      target. The pre-M8.8 path emitted a separate helper-IR
///      body per `.N` to keep wasm-ld from leaving them as imports.
///      Post-rewrite, every `.N` collapses to the same canonical
///      name — wasm-ld dedupes naturally.
///
///   2. **PLT-stub thunk mismatch** — caller's IR previously said
///      `sub_<stub_addr>` while the dep's lifted body was defined
///      as `sub_<real_addr>`. The pre-M8.8 path emitted a
///      `linkonce_odr` thunk `sub_<stub_addr> → musttail
///      sub_<real_addr>` to bridge. Post-rewrite, the chain map
///      pre-canonicalizes the caller's reference to `sub_<real_addr>`
///      and wasm-ld matches the dep's defn directly.
///
///   3. **Bare-`b imm26` tail-call shims** — same as PLT stubs but
///      detected at the byte level. SymbolTable's
///      `detect_arm64_tail_shims` populates the chain map for
///      these; the rewriter applies it.
///
///   4. **`lift_addr` arithmetic** — `branch_target_to_host_addr`
///      computed `host_target = fn_addr + (lift_target - lift_addr)`
///      at every callsite. The rewriter centralizes this so
///      downstream helpers operate in canonical-address space only.
///
/// Reference form:
///
///   `@sub_<hex>`     — entry defn, branch declare/call, etc.
///   `@sub_<hex>.<N>` — remill's per-call-site dedup variant
///
/// where `<hex>` is hexadecimal and `<N>` is decimal. The rewriter
/// matches both shapes and emits `@sub_<canonical_addr_hex>` (no
/// `.N`). Tokens with non-hex bodies are left intact (rare; some
/// helper-IR names like `@__remill_*` start with `@` but the
/// `sub_` prefix discriminator keeps them out of the match).
fn rewrite_sub_names_to_canonical(
    ir: &str,
    table: &symbol_table::SymbolTable,
    _fn_addr: usize,
    _lift_addr: u64,
) -> String {
    // M9-review: `lift_addr` is now `entry.synthetic_addr` (passed
    // to remill via `--address=`), so the IR's `sub_<hex>` values
    // are already in synthetic-address space. No more lift→host
    // arithmetic — just chase the synth chain to canonical synth
    // and emit the canonical hex. The fn_addr / lift_addr params
    // are kept in the signature for back-compat with call sites
    // but no longer used.
    let mut out = String::with_capacity(ir.len() + 128);
    let bytes = ir.as_bytes();
    let mut cursor = 0;
    while let Some(rel) = ir[cursor..].find("@sub_") {
        let abs = cursor + rel;
        out.push_str(&ir[cursor..abs]);
        // Now at "@sub_". Consume the prefix.
        let hex_start = abs + 5;
        let mut j = hex_start;
        while j < bytes.len() && bytes[j].is_ascii_hexdigit() {
            j += 1;
        }
        if j == hex_start {
            // Not a real `@sub_<hex>` — emit prefix verbatim and advance.
            out.push_str("@sub_");
            cursor = hex_start;
            continue;
        }
        let hex_end = j;
        let mut end = hex_end;
        // Optional `.<N>` decimal suffix — consume so the rewrite
        // collapses `.N` variants to the same canonical name.
        if end + 1 < bytes.len() && bytes[end] == b'.' {
            let suffix_start = end + 1;
            let mut m = suffix_start;
            while m < bytes.len() && bytes[m].is_ascii_digit() {
                m += 1;
            }
            if m > suffix_start {
                end = m;
            }
        }
        let hex = &ir[hex_start..hex_end];
        match usize::from_str_radix(hex, 16) {
            Ok(raw_synth) => {
                // Synth-space stub chain follow.
                let canonical_synth = table
                    .resolve_synth(raw_synth)
                    .unwrap_or(raw_synth);
                out.push_str(&format!("@sub_{:x}", canonical_synth));
            }
            Err(_) => {
                // Hex didn't parse — leave the original token intact.
                out.push_str(&ir[abs..end]);
            }
        }
        cursor = end;
    }
    out.push_str(&ir[cursor..]);
    out
}

// `BranchExternKind` + `classify_branch_extern` deleted in M8.8
// Stage 1. The SymbolTable's `FnClass` is the unified classification;
// `symbol_table::classify_for_name` does the suffix matching against
// `rust_alloc` / `az_call_indirect` / `az_resolve_callback` once at
// table build time. Every lift consumer reads
// `entry.classification` (already populated) instead of redoing the
// regex per call site.

/// Per-extern resolution info passed to [`emit_helper_ir`].
///
/// **M8.8 Stage 1**: the structure collapsed from three fields to two
/// once the SymbolTable handles PLT-stub chasing and `.N`-suffix
/// dedup. The helper IR now drives body shape directly off
/// `classification` (`SymFnClass`). `None` classification means the
/// table didn't know about this address — log loudly, emit no body,
/// let it become an env import.
#[derive(Debug, Clone)]
pub struct ResolvedBranchExtern {
    /// The `sub_<canonical_addr_hex>` name as it appears in the
    /// rewritten lifted IR. The `<hex>` is the symbol's canonical
    /// post-chain address.
    pub sym_name: String,
    /// SymbolTable classification, or `None` if the canonical
    /// address wasn't in the table (rare; surfaces a missed image).
    pub classification: Option<SymFnClass>,
}

fn run_tool(prog: &Path, args: &[&str], fn_name: &str) -> Result<(), TranspileError> {
    let out = Command::new(prog).args(args).output().map_err(|e| TranspileError {
        fn_name: fn_name.to_string(),
        reason: format!("spawn {}: {e}", prog.display()),
    })?;
    if !out.status.success() {
        return Err(TranspileError {
            fn_name: fn_name.to_string(),
            reason: format!(
                "{} failed: {}\nstderr: {}",
                prog.display(),
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        });
    }
    Ok(())
}
