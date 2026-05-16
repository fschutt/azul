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
        // Layout/VirtualView/etc. return structs larger than 16B
        // (StyledDom, VirtualViewReturn) — the aarch64 PCS uses a
        // hidden return-by-pointer in X8 for those. Today we
        // fall through to the Callback shape so the wrapper still
        // builds; the lifted body's actual return goes to wherever
        // X8 pointed in the caller's frame (not back through the
        // wrapper). M7+ work to add `Pcs::HiddenPtrReturn` + emit
        // the wrapper-side caller-allocated return buffer.
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
        }
    }
    (params.join(", "), prologue)
}

/// Build the return-type fragment and post-call return-read code from
/// the signature's return PCS.
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
        // Pair / hidden-ptr returns left for the M7 generalization
        // pass; canonical Callback shape never hits these.
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
        }
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
        let tools = self.tools(fn_name)?;
        std::fs::create_dir_all(&self.scratch_dir).map_err(|e| TranspileError {
            fn_name: fn_name.to_string(),
            reason: format!("scratch dir: {e}"),
        })?;

        // SAFETY: caller asserts `fn_addr` + `fn_size` cover a live
        // function in this process's .text. Reading is read-only and
        // bounded by `fn_size`; the slice is consumed before any other
        // operation that could remap memory.
        let bytes: Vec<u8> = unsafe {
            std::slice::from_raw_parts(fn_addr as *const u8, fn_size).to_vec()
        };

        let arch_tag = host_arch_tag().ok_or_else(|| TranspileError {
            fn_name: fn_name.to_string(),
            reason: "unsupported host architecture for remill (need aarch64 or x86_64)".into(),
        })?;

        // remill-lift takes hex on the cmdline and writes IR to a path.
        // Address `0x100000000`-class values are high enough that
        // remill's null-page guard doesn't bail; the caller varies
        // `lift_addr` per call to keep `sub_<hex>` symbol names unique
        // when multiple objects will be linked together.
        let stem = sanitize_filename(fn_name);
        let lifted_ir_path = self.scratch_dir.join(format!("{}.lifted.ll", stem));
        let hex = bytes_to_hex(&bytes);
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
                lifted_ir_path.to_str().expect("scratch path is utf-8"),
            ],
            fn_name,
        )?;

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
        let lifted_ir = std::fs::read_to_string(&lifted_ir_path).map_err(|e| TranspileError {
            fn_name: fn_name.to_string(),
            reason: format!("read lifted IR: {e}"),
        })?;
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
            inject_alwaysinline(&lifted_ir, lift_addr)
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
        // M7: parse branch-destination externs from the lifted IR.
        // remill emits each `bl <target>` whose destination is
        // outside the byte map as a `declare ptr @sub_<hex>(...)`.
        // The `<hex>` is the low 32 bits of the lift-space target
        // (sign-extended to i32 → relative offset from lift_addr).
        // To recover the host-binary address:
        //   host_target = fn_addr + signed_i32(hex)
        // dladdr-resolve each host_target → symbol name. The symbol
        // table tells us which framework function the lift was about
        // to call (`AzDom_addChild`, `AzString_clone`, …); M8 will
        // route those to typed externs from `azul-mini.wasm`. For
        // M7 we emit noop stubs so the imports disappear from the
        // produced WASM (currently the JS-side Proxy noops them at
        // load time; now the WASM is self-contained).
        let branch_sym_names = parse_extern_sub_declares(&lifted_ir);
        let mut resolved_branches: Vec<ResolvedBranchExtern> =
            Vec::with_capacity(branch_sym_names.len());
        for sym_name in &branch_sym_names {
            let kind = if let Some(host_addr) =
                branch_target_to_host_addr(sym_name, fn_addr, lift_addr)
            {
                let resolved = super::resolve_fn_ptr(host_addr);
                let kind = classify_branch_extern(&resolved.name);
                eprintln!(
                    "[azul-web]   intercept: {} → host=0x{:016x} = {} [{:?}]",
                    sym_name, host_addr, resolved.name, kind,
                );
                kind
            } else {
                eprintln!(
                    "[azul-web]   intercept: {} (lift-space addr parse failed)",
                    sym_name
                );
                BranchExternKind::Noop
            };
            resolved_branches.push(ResolvedBranchExtern {
                sym_name: sym_name.clone(),
                kind,
            });
        }
        let helper_ir = emit_helper_ir(lift_addr, sig, &resolved_branches, export_as);
        let helper_ir_path = self.scratch_dir.join(format!("{}.helper.ll", stem));
        std::fs::write(&helper_ir_path, &helper_ir).map_err(|e| TranspileError {
            fn_name: fn_name.to_string(),
            reason: format!("write helper IR: {e}"),
        })?;

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

        // llc → wasm32 object on the cleaned IR
        let obj_path = self.scratch_dir.join(format!("{}.o", stem));
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
    ) -> Result<Vec<u8>, TranspileError> {
        let tools = self.tools(output_stem)?;
        let wasm_path = self.scratch_dir.join(format!("{}.wasm", output_stem));
        let mut args: Vec<String> = vec![
            "--no-entry".to_string(),
            "--allow-undefined".to_string(),
            "--import-table".to_string(),
            // Initial memory: 2 MiB = 32 pages. Stack lives in low
            // addresses (~64 KiB), bump heap starts at 1 MiB (per
            // @__az_bump_ptr's initial value) and grows up. 2 MiB
            // gives us ~1 MiB of heap before exhaustion — enough for
            // hello-world's stateful demo. JS can grow via
            // memory.grow if needed later.
            "--initial-memory=2097152".to_string(),
            "-o".to_string(),
            wasm_path.to_string_lossy().into_owned(),
        ];
        for e in exports {
            args.push(format!("--export={}", e));
        }
        for p in objects {
            args.push(p.to_string_lossy().into_owned());
        }
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run_tool(tools.wasm_ld, &arg_refs, output_stem)?;
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

            eprintln!(
                "[azul-web]   transitive[{}]: lifting {} addr=0x{:016x} \
                 size={} export_as={}",
                lifted_count, name, addr, size, export_as
            );

            let obj = self.produce_object_for(
                &name, addr, size, &sig, &export_as, lift_addr,
            )?;
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
            for sym in parse_extern_sub_declares(&lifted_ir) {
                let Some(host_addr) =
                    branch_target_to_host_addr(&sym, addr, lift_addr)
                else { continue; };
                if visited.contains(&host_addr) {
                    continue;
                }
                let resolved = super::resolve_fn_ptr(host_addr);
                let kind = classify_branch_extern(&resolved.name);
                if !matches!(kind, BranchExternKind::Noop) {
                    // Known leaf — helper IR provides the body.
                    continue;
                }
                if resolved.name.starts_with("cb_") {
                    // dladdr couldn't resolve a real symbol —
                    // dont' attempt to lift garbage bytes.
                    continue;
                }
                queue.push_back(TransitiveLiftTarget::Dep {
                    name: resolved.name,
                    addr: host_addr,
                    size: resolved.size,
                });
            }
        }

        eprintln!(
            "[azul-web] transitive lift complete: {} functions lifted, {} unique exports",
            visited.len(),
            exports.len()
        );

        let bytes =
            self.link_objects_to_wasm(&object_paths, &exports, "transitive-lift")?;
        Ok(WasmModule {
            content_hash: super::fnv1a64_hex(&bytes),
            bytes,
            exports,
            imports_from_mini: Vec::new(),
        })
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
    ) -> Result<WasmModule, TranspileError> {
        // Per-widget callback lifts use the canonical Callback shape +
        // export under the stable name `callback` (so loader.js can
        // dispatch without per-callback name lookups).
        let sig = signature_for_callback_kind("Callback");
        self.pipeline_single(fn_name, fn_addr, fn_size, &sig, super::WASM_CALLBACK_EXPORT)
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
        for (i, ((name, addr, size), sig)) in symbols.iter().zip(sigs.iter()).enumerate() {
            // Unique lift_addr per function so each lifted module's
            // top-level `sub_<lift_addr_hex>` is a distinct symbol.
            // 0x1000 stride is much larger than any plausible function
            // (256B read window today), so back-references stay within
            // their own lift's namespace.
            let lift_addr = 0x100000000_u64 + (i as u64) * 0x1000;
            eprintln!(
                "[azul-web]   eventloop[{i}]: lifting {} addr=0x{:016x} size={} lift_addr=0x{:x}",
                name, addr, size, lift_addr,
            );
            let obj = self.produce_object_for(name, *addr, *size, sig, name, lift_addr)?;
            object_paths.push(obj);
            exports.push(name.clone());
        }

        let bytes = self.link_objects_to_wasm(&object_paths, &exports, "azul-mini")?;
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
    //
    // `@__bump_ptr` is declared `linkonce_odr` so the wasm-ld link
    // step over multiple object files (the azul-mini eventloop
    // case) dedupes to one shared global — every AzStartup_* shares
    // the same heap. Initial offset 65536 (64 KiB) leaves the wasm
    // stack guard zone alone; subsequent grow is fine because
    // azul-mini.wasm imports `memory` from JS with growth allowed.
    let mut branch_stubs = String::new();
    for ext in branch_externs {
        match ext.kind {
            BranchExternKind::Noop => {
                branch_stubs.push_str(&format!(
                    "define linkonce_odr ptr @{sym}(ptr %state, i64 %pc, ptr %memory) alwaysinline {{ \
                     ret ptr %memory }}\n",
                    sym = ext.sym_name,
                ));
            }
            BranchExternKind::RustAlloc | BranchExternKind::RustAllocZeroed => {
                // size = State.X0; align ignored (8-byte minimum).
                // Bump @__bump_ptr; write old value to X0 (return).
                branch_stubs.push_str(&format!(
                    "; bump-allocator body for {label}\n\
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
                    label = match ext.kind {
                        BranchExternKind::RustAlloc => "__rust_alloc",
                        BranchExternKind::RustAllocZeroed => "__rust_alloc_zeroed",
                        _ => unreachable!(),
                    },
                    n = ext.sym_name, // SSA-name suffix; sym names are unique per call site
                    x0_off = x0_off,
                ));
            }
            BranchExternKind::AzCallIndirect => {
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
                    n = ext.sym_name,
                    x0_off = x0_off,
                    x1_off = x0_off + 16,
                    x2_off = x0_off + 32,
                    x3_off = x0_off + 48,
                ));
            }
            BranchExternKind::AzResolveCallback => {
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
                    n = ext.sym_name,
                    x0_off = x0_off,
                ));
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
/// Plain text-grep is robust enough for this — remill's emitted IR
/// uses a consistent declaration shape per `sub_<hex>` extern, one
/// per line, with no comment artifacts that could trip a naïve
/// matcher.
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

/// Recover the host-binary address a `sub_<hex>` branch destination
/// points at. remill names branch destinations after the full
/// lift-space target address, formatted as hex; the offset from the
/// lift_addr base maps to the host binary as:
///
///   host_target = fn_addr + (lift_space_target - lift_addr)
///
/// Note: remill sometimes emits only the low 32 bits (when the target
/// is a relative branch within the lifted byte map) and sometimes the
/// full 64-bit lift-space address (for cross-module / far calls like
/// `__rust_alloc`). Treating the hex as `u64` and computing
/// `wrapping_sub(lift_addr)` works in both cases: a low-32 hex like
/// `0xfffffda4` minus `0x100000000` wraps to `-0x25c`, which is the
/// expected backward branch offset; a 9+-char hex like
/// `0x1000c3940` minus `0x100000000` is `0xc3940`, the forward
/// (cross-module) offset.
///
/// Returns `None` if the hex doesn't parse as u64.
fn branch_target_to_host_addr(
    sym_name: &str,
    fn_addr: usize,
    lift_addr: u64,
) -> Option<usize> {
    let hex = sym_name.strip_prefix("sub_")?;
    let raw = u64::from_str_radix(hex, 16).ok()?;
    let offset = raw.wrapping_sub(lift_addr) as i64 as isize;
    Some((fn_addr as isize).wrapping_add(offset) as usize)
}

/// Classification of a resolved branch-extern symbol — drives the
/// helper-IR body emit choice (noop vs. bump allocator vs. ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchExternKind {
    /// Default: emit a body that returns the memory token unchanged
    /// (no state mutation, no allocation).
    Noop,
    /// `__rust_alloc(size, align) -> ptr` — emit a body that reads
    /// `size` from State.X0, bumps `@__bump_ptr` by it (8-byte
    /// aligned), writes the old `@__bump_ptr` value back to State.X0.
    RustAlloc,
    /// `__rust_alloc_zeroed` — same as RustAlloc since wasm linear
    /// memory is zero-initialized and bump never reuses memory.
    RustAllocZeroed,
    /// `__az_call_indirect(table_idx, refany_lo, refany_hi, info_ptr)
    /// -> i32` — emit a body that reads the four args from State.X0-X3
    /// and does a wasm `call_indirect` via `inttoptr i32 %tidx to ptr`
    /// + a typed `call i32 %fn(i64, i64, i32)`. The LLVM wasm backend
    /// lowers the inttoptr+call combination to `call_indirect` using
    /// `__indirect_function_table` (imported from JS).
    AzCallIndirect,
    /// `__az_resolve_callback(cb_fn_addr_u64) -> i32` — wasm-side
    /// import resolved by JS. Helper IR doesn't emit a body; instead
    /// it leaves the lifted-site call hooked to the JS import.
    AzResolveCallback,
}

/// Classify a dladdr-resolved symbol name into one of the
/// known-special bodies. Returns `Noop` for any name not in the
/// special set.
///
/// Symbol-name shapes we have to handle:
///   - Bare `__rust_alloc` (some Linux setups, custom builds).
///   - macOS-prefixed `___rust_alloc` (leading underscore added).
///   - Rust v0 mangled name like
///     `_RNvCs5r5JX3umY3f_7___rustc12___rust_alloc` where the bare
///     name is the trailing length-prefixed identifier.
///
/// Suffix-matching after trimming leading underscores covers all
/// three shapes. `rust_alloc_zeroed` is checked first because
/// `ends_with("rust_alloc")` would otherwise match the zeroed name's
/// substring. `rust_no_alloc_shim_is_unstable_v2` (the rustc-emit
/// shim that signals "allocator is real, not the no-op stub") does
/// NOT contain `rust_alloc` as a suffix so it falls through to Noop
/// correctly.
pub fn classify_branch_extern(resolved_name: &str) -> BranchExternKind {
    let s = resolved_name.trim_start_matches('_');
    if s.ends_with("rust_alloc_zeroed") {
        BranchExternKind::RustAllocZeroed
    } else if s.ends_with("rust_alloc") {
        BranchExternKind::RustAlloc
    } else if s == "az_call_indirect" || s.ends_with("az_call_indirect") {
        BranchExternKind::AzCallIndirect
    } else if s == "az_resolve_callback" || s.ends_with("az_resolve_callback") {
        BranchExternKind::AzResolveCallback
    } else {
        BranchExternKind::Noop
    }
}

/// Per-extern resolution info passed to [`emit_helper_ir`] so it can
/// emit per-symbol bodies (bump allocator for `__rust_alloc`, noop
/// for everything else).
#[derive(Debug, Clone)]
pub struct ResolvedBranchExtern {
    /// The raw `sub_<hex>` name as it appears in the lifted IR's
    /// `declare` line.
    pub sym_name: String,
    /// Classification driving the body shape. Determined by
    /// dladdr-resolving `sym_name` then calling
    /// [`classify_branch_extern`] on the result.
    pub kind: BranchExternKind,
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
