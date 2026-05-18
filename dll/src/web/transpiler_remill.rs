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
    /// `wasm-ld` chain. Requires BOTH the build-time feature
    /// (`web-transpiler-static` statically links the library bodies
    /// into libazul.dylib) AND the runtime opt-in (`AZ_NATIVE_REMILL=1`).
    /// The cfg-gate keeps the env-var check from accidentally enabling
    /// a path that wouldn't link.
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
        let bytes: Vec<u8> = unsafe {
            std::slice::from_raw_parts(fn_addr as *const u8, fn_size).to_vec()
        };
        let arch_tag = host_arch_tag().ok_or_else(|| TranspileError {
            fn_name: fn_name.to_string(),
            reason: "unsupported host architecture for remill (need aarch64 or x86_64)".into(),
        })?;
        let stem = sanitize_filename(fn_name);
        let lifted_ir_path = self.scratch_dir.join(format!("{}.lifted.ll", stem));
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
        std::fs::read_to_string(&lifted_ir_path).map_err(|e| TranspileError {
            fn_name: fn_name.to_string(),
            reason: format!("read lifted IR: {e}"),
        })
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

        // The entry's defn after rewrite uses fn_addr as its hex
        // (since the dispatcher pre-chases the chain, fn_addr IS the
        // canonical address). Pass fn_addr to inject_alwaysinline so
        // it finds the right `define` line.
        let canonical_entry_addr = symbol_table::get()
            .and_then(|t| t.canonical_addr_for(fn_addr))
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
        let patched_ir = if export_as.starts_with("AzStartup_") {
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
        let branch_sym_names = parse_extern_sub_declares(&lifted_ir);
        let mut resolved_branches: Vec<ResolvedBranchExtern> =
            Vec::with_capacity(branch_sym_names.len());
        for sym_name in &branch_sym_names {
            let addr = parse_sub_hex_as_addr(sym_name).unwrap_or(0);
            let classification = symbol_table::get()
                .and_then(|t| t.lookup(addr))
                .map(|e| e.classification);
            eprintln!(
                "[azul-web]   intercept: {} → addr=0x{:016x} class={:?}",
                sym_name, addr, classification,
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
                    "-O2",
                    "-S",
                    linked_ir_path.to_str().expect("scratch path is utf-8"),
                    "-o",
                    opt_ir_path.to_str().expect("scratch path is utf-8"),
                ],
                fn_name,
            )?;

            run_tool(
                tools.llc,
                &[
                    "-mtriple=wasm32-unknown-unknown",
                    "-filetype=obj",
                    "-O2",
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
    ) -> Result<Vec<u8>, TranspileError> {
        // M8.9 Phase 2b: native lld::wasm path when AZ_NATIVE_REMILL=1.
        // The C++ wrapper writes each obj to a per-call temp dir
        // internally (lld's API takes file paths, not memory buffers),
        // then reads the output wasm back into a heap buffer. From
        // here it's the same input/output shape as the subprocess
        // path — bytes in, bytes out.
        //
        // Initial memory raised to 16 MiB (was 2 MiB) — the bump
        // allocator never frees, so any non-trivial Vec/Box usage in
        // a lifted layout cb (hello-world's full StyledDom build →
        // ~few hundred KiB of NodeData + CssVec) eats through the
        // 1 MiB heap quickly and traps with "memory access out of
        // bounds" at the first overflow. 16 MiB gives ~15 MiB of
        // bump heap before the limit. JS can `memory.grow()` past
        // that, but a higher initial avoids a grow per layout cb.
        let initial_memory_bytes: u32 = 16 * 1024 * 1024;
        let import_memory = matches!(memory_mode, MemoryMode::ImportMemory);
        // import_table mirrors the subprocess `--import-table` flag —
        // funcref table is JS-owned (sized + populated with per-cb
        // wasm `callback` exports at instantiate-time). Both per-cb /
        // per-layout (ImportMemory) and azul-mini.wasm (OwnMemory)
        // need this for __az_call_indirect to work.
        let import_table = true;
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
                // Run wasm-opt -Oz on the linked output for the same
                // size win the subprocess path gets below.
                let pre_opt_path = self.scratch_dir.join(format!("{}.pre-opt.wasm", output_stem));
                let _ = std::fs::write(&pre_opt_path, &linked);
                if let Some(opt) = postprocess_wasm_opt(&pre_opt_path, output_stem) {
                    return Ok(opt);
                }
                return Ok(linked);
            }
        }
        let tools = self.tools(output_stem)?;
        let wasm_path = self.scratch_dir.join(format!("{}.wasm", output_stem));
        let mut args: Vec<String> = vec![
            "--no-entry".to_string(),
            "--allow-undefined".to_string(),
            // --gc-sections strips unreachable functions (e.g. dead
            // lifted bodies the wrapper doesn't transitively call).
            // --strip-all removes debug/name/producer custom sections.
            // --lto-O2 enables cross-object LTO so dead code that
            // crosses .o boundaries also gets DCE'd.
            "--gc-sections".to_string(),
            "--strip-all".to_string(),
            "--lto-O2".to_string(),
        ];
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
        let final_bytes = postprocess_wasm_opt(&wasm_path, output_stem);
        if let Some(b) = final_bytes {
            return Ok(b);
        }
        std::fs::read(&wasm_path).map_err(|e| TranspileError {
            fn_name: output_stem.to_string(),
            reason: format!("read {}: {e}", wasm_path.display()),
        })
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
    /// **Depth limit**: hard-capped at `MAX_RECURSIVE_DEPTH`
    /// functions. If hit, returns an error so a runaway chain
    /// doesn't lock up the server forever.
    ///
    /// **Skipped externs**: dladdr fallbacks (`cb_<hex>` names),
    /// known-leaf classifications (RustAlloc, AzCallIndirect, etc.)
    /// don't recurse — they get bodies from helper IR. Unresolved
    /// externs become noop stubs.
    pub fn lift_with_transitive_deps(
        &self,
        roots: Vec<TransitiveLiftRoot>,
    ) -> Result<WasmModule, TranspileError> {
        // M8.9 Phase 3b: in the native pipeline, pre-walk the dep
        // graph via ARM64 bytes-scan to discover the full set
        // upfront, then batch-lift everything in one call. Saves
        // (N-1)×LoadArchSemantics cost (~30 ms each) — for the
        // hello-world transitive on_click (~12 fns) that's ~330 ms
        // off the first request.
        if self.use_native_remill() {
            #[cfg(feature = "web-transpiler-static")]
            return self.lift_with_transitive_deps_batched(roots);
        }
        self.lift_with_transitive_deps_sequential(roots)
    }

    fn lift_with_transitive_deps_sequential(
        &self,
        roots: Vec<TransitiveLiftRoot>,
    ) -> Result<WasmModule, TranspileError> {
        // Hard cap on the number of functions a single root's
        // transitive closure can pull in. Bumped from 64 → 256 in
        // M8.8 once exact-size lifts surface the full layout-cb
        // dependency graph (40+ azul-css / azul-core / azul-layout
        // helpers around CssVec/DomVec/Dom clones+drops). With the
        // per-canonical-addr `object_cache` below, repeat lifts of
        // the same dep across multiple callbacks are O(1), so the
        // cap is about call-graph fan-out, not throughput.
        const MAX_RECURSIVE_DEPTH: usize = 256;

        let mut visited: HashSet<usize> = HashSet::new();
        let mut queue: VecDeque<TransitiveLiftTarget> = VecDeque::new();
        let mut object_paths: Vec<PathBuf> = Vec::new();
        let mut exports: Vec<String> = Vec::new();

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
            if lifted_count > MAX_RECURSIVE_DEPTH {
                return Err(TranspileError {
                    fn_name: name,
                    reason: format!(
                        "transitive lift exceeded {} functions — runaway recursion?",
                        MAX_RECURSIVE_DEPTH
                    ),
                });
            }

            // lift_addr = native addr so caller/callee sub_<hex>
            // symbols align without rewriting.
            let lift_addr = addr as u64;

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
                let Some(canonical_addr) = parse_sub_hex_as_addr(&sym) else {
                    eprintln!("[azul-web]     dep: {} (canonical hex parse failed)", sym);
                    continue;
                };
                let entry = match symbol_table::get().and_then(|t| t.lookup(canonical_addr)) {
                    Some(e) => e.clone(),
                    None => {
                        eprintln!(
                            "[azul-web]     dep: {} addr=0x{:016x} not in SymbolTable — skipping",
                            sym, canonical_addr,
                        );
                        continue;
                    }
                };
                let already_visited = visited.contains(&entry.canonical_addr);
                eprintln!(
                    "[azul-web]     dep: {} → resolved={}@0x{:016x} class={:?} visited={}",
                    sym,
                    entry.canonical_name,
                    entry.canonical_addr,
                    entry.classification,
                    already_visited,
                );
                if already_visited {
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

        eprintln!(
            "[azul-web] transitive lift complete: {} functions lifted, {} unique exports",
            visited.len(),
            exports.len()
        );

        let bytes = self.link_objects_to_wasm(
            &object_paths,
            &exports,
            "transitive-lift",
            MemoryMode::ImportMemory,
        )?;
        Ok(WasmModule {
            content_hash: super::fnv1a64_hex(&bytes),
            bytes,
            exports,
            imports_from_mini: Vec::new(),
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
    ) -> Result<WasmModule, TranspileError> {
        const MAX_RECURSIVE_DEPTH: usize = 256;
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
            if targets.len() >= MAX_RECURSIVE_DEPTH {
                return Err(TranspileError {
                    fn_name: name,
                    reason: format!(
                        "transitive lift exceeded {} functions — runaway recursion?",
                        MAX_RECURSIVE_DEPTH
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
                    unsafe {
                        std::slice::from_raw_parts(t.addr as *const u8, t.size).to_vec()
                    }
                })
                .collect();
            let items: Vec<(u64, &[u8])> = to_lift_idx
                .iter()
                .zip(bytes_vec.iter())
                .map(|(&i, b)| (targets[i].addr as u64, b.as_slice()))
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
            for (&i, lifted_ir) in to_lift_idx.iter().zip(per_fn_irs.iter()) {
                let t = &targets[i];
                let lift_addr = t.addr as u64;
                let obj = self.produce_object_from_lifted_ir(
                    &t.name, t.addr, lift_addr, &t.sig, &t.export_as, lifted_ir,
                )?;
                self.object_cache
                    .lock()
                    .unwrap()
                    .insert((t.addr, t.export_as.clone()), obj.clone());
                object_paths.push(obj);
                exports.push(t.export_as.clone());
            }
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
            "transitive-lift",
            MemoryMode::ImportMemory,
        )?;
        Ok(WasmModule {
            content_hash: super::fnv1a64_hex(&bytes),
            bytes,
            exports,
            imports_from_mini: Vec::new(),
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

/// Memory-import vs own-memory selection for
/// [`RemillTranspiler::link_objects_to_wasm`]. Per-cb / per-layout
/// wasms import `env.memory` so they share linear address space
/// with the mini wasm; the mini wasm itself ships an exported
/// `memory` that the JS bootstrap routes to all other wasms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoryMode {
    /// `--initial-memory=N` — wasm-ld declares + exports its own
    /// `memory`. Used by the mini wasm (the source of truth for
    /// shared memory).
    OwnMemory,
    /// `--import-memory` — wasm-ld emits an import for `env.memory`
    /// instead of declaring its own. JS supplies mini's exported
    /// memory at instantiate time.
    ImportMemory,
}

/// Specifies a root function for [`RemillTranspiler::lift_with_transitive_deps`].
#[derive(Debug, Clone)]
pub struct TransitiveLiftRoot {
    pub fn_name: String,
    pub fn_addr: usize,
    pub fn_size: usize,
    pub sig: CallbackSignature,
    pub export_as: String,
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
        let mut sigs: Vec<CallbackSignature> = Vec::with_capacity(symbols.len());
        for (name, _, _) in symbols {
            let sig = signature_for_eventloop_fn(name).ok_or_else(|| TranspileError {
                fn_name: name.clone(),
                reason: format!(
                    "no entry in signature_for_eventloop_fn for {} — add it before \
                     listing in EVENTLOOP_SYMBOLS",
                    name
                ),
            })?;
            sigs.push(sig);
        }

        let mut object_paths: Vec<PathBuf> = Vec::with_capacity(symbols.len());
        let mut exports: Vec<String> = Vec::with_capacity(symbols.len());

        // M8.9 Phase 3a: in the native pipeline, batch-lift every
        // eventloop fn in ONE az_remill_lift_batch call. Shares
        // LoadArchSemantics (~30 ms) across all items — per-fn lift
        // cost drops from ~50 ms to ~5 ms. The TraceManager spans
        // the union of all byte ranges so inter-eventloop `bl`
        // (AzStartup_hydrate → AzStartup_alloc) resolves to the
        // lifted body in the same batched manager rather than as an
        // out-of-range extern. The per-fn IR strings returned by the
        // batch then feed produce_object_from_lifted_ir for the
        // post-lift compile.
        //
        // Subprocess path keeps the per-fn loop — each subprocess
        // spawn pays the LoadArchSemantics cost regardless, and
        // there's no batched API in remill-lift-17.
        if self.use_native_remill() {
            #[cfg(feature = "web-transpiler-static")]
            {
                let arch_tag = host_arch_tag().ok_or_else(|| TranspileError {
                    fn_name: "azul-mini".into(),
                    reason: "unsupported host architecture".into(),
                })?;
                let bytes_vec: Vec<Vec<u8>> = symbols
                    .iter()
                    .map(|(_, addr, size)| unsafe {
                        std::slice::from_raw_parts(*addr as *const u8, *size).to_vec()
                    })
                    .collect();
                let items: Vec<(u64, &[u8])> = symbols
                    .iter()
                    .zip(bytes_vec.iter())
                    .map(|((_, addr, _), b)| (*addr as u64, b.as_slice()))
                    .collect();
                let t0 = std::time::Instant::now();
                let per_fn_irs = super::native_remill::lift_batch(
                    arch_tag,
                    host_os_tag(),
                    &items,
                )
                .map_err(|e| TranspileError {
                    fn_name: "azul-mini".into(),
                    reason: format!("native batched lift: {}", e),
                })?;
                eprintln!(
                    "[azul-web]   eventloop: batched lift of {} items in {:?}",
                    items.len(),
                    t0.elapsed(),
                );
                for (((name, addr, size), sig), lifted_ir) in
                    symbols.iter().zip(sigs.iter()).zip(per_fn_irs.iter())
                {
                    let lift_addr = *addr as u64;
                    eprintln!(
                        "[azul-web]   eventloop: post-lift {} addr=0x{:016x} size={}",
                        name, addr, size,
                    );
                    let obj = self.produce_object_from_lifted_ir(
                        name, *addr, lift_addr, sig, name, lifted_ir,
                    )?;
                    object_paths.push(obj);
                    exports.push(name.clone());
                }
            }
        } else {
            for (i, ((name, addr, size), sig)) in symbols.iter().zip(sigs.iter()).enumerate() {
                // CRITICAL: lift_addr MUST equal the native addr so
                // inter-fn `bl` targets align. AzStartup_hydrate doing
                // `bl AzStartup_alloc` lifts to `call sub_<native_addr_of_alloc>`;
                // if alloc's body is at `sub_<some_synthetic>` the linker
                // can't connect them — the helper IR emits a noop stub
                // and the cross-fn call silently does nothing. Aligning
                // lift_addr with native_addr means alloc's body is
                // emitted as `sub_<native_addr_of_alloc>`, matching
                // every bl target from any other lifted eventloop fn.
                //
                // (Same mechanism as `lift_with_transitive_deps` for
                // per-cb / per-layout wasms.)
                let lift_addr = *addr as u64;
                eprintln!(
                    "[azul-web]   eventloop[{i}]: lifting {} addr=0x{:016x} size={} lift_addr=0x{:x}",
                    name, addr, size, lift_addr,
                );
                let obj = self.produce_object_for(name, *addr, *size, sig, name, lift_addr)?;
                object_paths.push(obj);
                exports.push(name.clone());
            }
        }

        let bytes = self.link_objects_to_wasm(
            &object_paths,
            &exports,
            "azul-mini",
            MemoryMode::OwnMemory,
        )?;
        Ok(WasmModule {
            content_hash: super::fnv1a64_hex(&bytes),
            bytes,
            exports,
            imports_from_mini: Vec::new(),
        })
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

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
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
    // SP-relative spills. The body's prologue decrements SP by ~96
    // and stores X29/X30 at SP-relative addresses; a 4 KiB buffer
    // leaves headroom for deeper call trees. The SP register holds
    // the address of the *top* of this buffer (i64 of a wasm32
    // pointer); SP arithmetic decrements toward lower addresses
    // within the buffer.
    let stack_size: u64 = 4096;
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
            // BumpAlloc: __rust_alloc / __rust_alloc_zeroed body.
            Some(SymFnClass::BumpAlloc) => {
                branch_stubs.push_str(&format!(
                    "; bump-allocator body for {sym}\n\
                     define linkonce_odr ptr @{sym}(ptr %state, i64 %pc, ptr %memory) alwaysinline {{\n  \
                       %x0_p_{n} = getelementptr inbounds i8, ptr %state, i64 {x0_off}\n  \
                       %size_{n} = load i64, ptr %x0_p_{n}, align 8\n  \
                       %size_a_{n} = add i64 %size_{n}, 7\n  \
                       %size_aligned_{n} = and i64 %size_a_{n}, -8\n  \
                       %old_{n} = load i32, ptr @__az_bump_ptr, align 4\n  \
                       %old_i64_{n} = zext i32 %old_{n} to i64\n  \
                       %new_i64_{n} = add i64 %old_i64_{n}, %size_aligned_{n}\n  \
                       %new_{n} = trunc i64 %new_i64_{n} to i32\n  \
                       store i32 %new_{n}, ptr @__az_bump_ptr, align 4\n  \
                       store i64 %old_i64_{n}, ptr %x0_p_{n}, align 8\n  \
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
            // returns memory unchanged + leaves State.X0 untouched
            // (the lift's call site reads back whatever was there
            // before, typically junk; safe for void/error returns).
            Some(SymFnClass::Leaf) => {
                branch_stubs.push_str(&format!(
                    "; Leaf body for {sym} — noop, returns memory unchanged\n\
                     define linkonce_odr ptr @{sym}(ptr %state, i64 %pc, ptr %memory) alwaysinline {{\n  \
                       ret ptr %memory\n\
                     }}\n",
                    sym = ext.sym_name,
                ));
            }
            // NeverLift: AzApp_run + other server-entry-points. Should
            // never appear in a cb body; emit a trap so we hear about
            // it loudly if it ever fires through.
            Some(SymFnClass::NeverLift) => {
                branch_stubs.push_str(&format!(
                    "; NeverLift trap for {sym}\n\
                     define linkonce_odr ptr @{sym}(ptr %state, i64 %pc, ptr %memory) {{\n  \
                       unreachable\n\
                     }}\n",
                    sym = ext.sym_name,
                ));
            }
            // No classification: the SymbolTable didn't have this
            // address. Indicates an image we didn't enumerate, or a
            // dynamically-resolved address. Leave as extern so
            // wasm-ld emits an `env.sub_<hex>` import; the M8.8
            // verification flags this as a coverage gap.
            None => {
                eprintln!(
                    "[azul-web]   unclassified extern: {} — emitting env import",
                    ext.sym_name
                );
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
    let bump_global = "@__az_bump_ptr = linkonce_odr global i32 1048576, align 4\n\
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
  ret ptr %memory
}}
define linkonce_odr ptr @__remill_error(ptr %state, i64 %pc, ptr %memory) alwaysinline {{
  ret ptr %memory
}}
define linkonce_odr i8 @__remill_read_memory_8(ptr %memory, i64 %addr) alwaysinline {{
  %p = inttoptr i64 %addr to ptr
  %v = load i8, ptr %p, align 1
  ret i8 %v
}}
define linkonce_odr i16 @__remill_read_memory_16(ptr %memory, i64 %addr) alwaysinline {{
  %p = inttoptr i64 %addr to ptr
  %v = load i16, ptr %p, align 2
  ret i16 %v
}}
define linkonce_odr i32 @__remill_read_memory_32(ptr %memory, i64 %addr) alwaysinline {{
  %p = inttoptr i64 %addr to ptr
  %v = load i32, ptr %p, align 4
  ret i32 %v
}}
define linkonce_odr i64 @__remill_read_memory_64(ptr %memory, i64 %addr) alwaysinline {{
  %p = inttoptr i64 %addr to ptr
  %v = load i64, ptr %p, align 8
  ret i64 %v
}}
define linkonce_odr ptr @__remill_write_memory_8(ptr %memory, i64 %addr, i8 %val) alwaysinline {{
  %p = inttoptr i64 %addr to ptr
  store i8 %val, ptr %p, align 1
  ret ptr %memory
}}
define linkonce_odr ptr @__remill_write_memory_16(ptr %memory, i64 %addr, i16 %val) alwaysinline {{
  %p = inttoptr i64 %addr to ptr
  store i16 %val, ptr %p, align 2
  ret ptr %memory
}}
define linkonce_odr ptr @__remill_write_memory_32(ptr %memory, i64 %addr, i32 %val) alwaysinline {{
  %p = inttoptr i64 %addr to ptr
  store i32 %val, ptr %p, align 4
  ret ptr %memory
}}
define linkonce_odr ptr @__remill_write_memory_64(ptr %memory, i64 %addr, i64 %val) alwaysinline {{
  %p = inttoptr i64 %addr to ptr
  store i64 %val, ptr %p, align 8
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

; Callback kind: {kind}. Wrapper synthesized from
; the matching `CallbackSignature`. PCS table at the top of
; `transpiler_remill.rs`. Exports as `{export_as}` so wasm-ld can
; surface it to the loader / JS.
define {ret_ty} @{export_as}({params}) {{
  ; State: register-file storage. Strictly aliased (no `ptrtoint`
  ; ever taken of `%state_buf`), so opt -O2's SROA can promote it
  ; into individual scalar slots after the lifted body inlines.
  %state_buf = alloca [{state_size} x i8], align 16
  ; Stack scratch: SP-relative spills land here. Its address IS
  ; ptrtoint'd (for the initial SP value), so SROA can't promote
  ; this one — but it's small and self-contained.
  %stack_buf = alloca [{stack_size} x i8], align 16

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
    fn_addr: usize,
    lift_addr: u64,
) -> String {
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
        match u64::from_str_radix(hex, 16) {
            Ok(raw) => {
                // Map lift-space hex → host addr.
                let offset = raw.wrapping_sub(lift_addr) as i64 as isize;
                let host_addr = (fn_addr as isize).wrapping_add(offset) as usize;
                let canonical_addr = table
                    .canonical_addr_for(host_addr)
                    .unwrap_or(host_addr);
                out.push_str(&format!("@sub_{:x}", canonical_addr));
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
