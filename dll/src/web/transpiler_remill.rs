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

    /// Run the three-stage pipeline on a single function: peek bytes from
    /// the running .text, lift to IR via remill, compile to a wasm32
    /// object, and link to a self-contained `.wasm` module.
    fn pipeline_single(
        &self,
        fn_name: &str,
        fn_addr: usize,
        fn_size: usize,
    ) -> Result<WasmModule, TranspileError> {
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
        // Address `0x100000000` matches the blueprint experiment — picks
        // a high virtual address so remill's null-page guard doesn't
        // bail out.
        let lift_addr: u64 = 0x100000000;
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
        let patched_ir = inject_alwaysinline(&lifted_ir, lift_addr);
        let patched_ir_path = self.scratch_dir.join(format!("{}.patched.ll", stem));
        std::fs::write(&patched_ir_path, &patched_ir).map_err(|e| TranspileError {
            fn_name: fn_name.to_string(),
            reason: format!("write patched IR: {e}"),
        })?;

        // Pick the wrapper signature for this callback. Today the
        // discovery side doesn't carry the typedef name through, so
        // we default to the canonical `Callback` shape — correct for
        // every widget OnClick/Hover/etc. callback (they all match
        // `fn(AzRefAny, AzCallbackInfo) -> AzUpdate`). M7+ extends
        // `DiscoveredCallback` with a typedef tag set at the
        // attachment site (set_on_toggle / set_on_value_change /
        // layout_callback) and we'd plumb that through here.
        let sig = signature_for_callback_kind("Callback");
        let helper_ir = emit_helper_ir(lift_addr, &sig);
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

        // wasm-ld → final module. Export `callback` (the wrapper) — the
        // raw `sub_<addr>` is now inlined away.
        let wasm_path = self.scratch_dir.join(format!("{}.wasm", stem));
        run_tool(
            tools.wasm_ld,
            &[
                "--no-entry",
                "--export=callback",
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
            // The M6 pipeline replaces the raw `sub_<addr>` export
            // with a wrapper exported as `callback` — stable name
            // loader.js dispatches under.
            exports: vec!["callback".to_string()],
            // TODO(M7/WB1.3): scan the lifted IR for external `call`s
            // and surface them as imports from azul-mini.wasm. For
            // now, leave empty so the caller treats the module as
            // self-contained.
            imports_from_mini: Vec::new(),
        })
    }
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
        self.pipeline_single(fn_name, fn_addr, fn_size)
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

fn discover_remill_lift() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("REMILL_LIFT_BIN") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    let candidates = [
        // Co-located with the workspace via the build_remill.sh script.
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
fn emit_helper_ir(lift_addr: u64, sig: &CallbackSignature) -> String {
    // SP register slot in the State struct (aarch64-specific).
    let sp_off: u64 = 1040;
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

declare ptr @sub_{lift_addr_hex}(ptr noalias, i64, ptr noalias)
declare void @llvm.memset.p0.i64(ptr nocapture writeonly, i8, i64, i1 immarg)

; Callback kind: {kind}. Wrapper synthesized from
; `signature_for_callback_kind({kind:?})` — see top of
; `transpiler_remill.rs` for the PCS table.
define {ret_ty} @callback({params}) {{
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
    )
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
