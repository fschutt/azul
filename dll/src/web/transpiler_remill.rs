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

        let helper_ir = emit_helper_ir(lift_addr);
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
///   2. A `callback` wrapper that allocates a 4096-byte stack buffer
///      for the State struct, zeroes it, seeds the X0/X1/X2 register
///      slots from the wrapper's `i64` args, points SP at the top of
///      the buffer (so the lift's SP-relative spills land in-bounds),
///      calls `sub_<lift_addr>(state_buf, lift_addr, null)`, and
///      reads X0 back as the i32 return.
///
/// The wrapper's signature `(i64, i64, i64) -> i32` matches the
/// aarch64 PCS layout of `extern "C" fn(AzRefAny, AzCallbackInfo)
/// -> AzUpdate`:
///   X0/X1 = AzRefAny (16-byte struct in two 8-byte regs)
///   X2    = AzCallbackInfo pointer (struct >16B passed by ptr)
///   W0    = AzUpdate (4-byte enum, lo half of X0)
fn emit_helper_ir(lift_addr: u64) -> String {
    // Per-arch State field offsets. Hardcoded for aarch64 to match the
    // GEPs the lifted IR emits (X0 at +544, X1 at +560, X2 at +576,
    // SP at +1040). x86_64 will need its own offsets when we add
    // x86_64 host support.
    let (x0_off, x1_off, x2_off, sp_off) = (544u64, 560u64, 576u64, 1040u64);
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

define i32 @callback(i64 %x0_arg, i64 %x1_arg, i64 %x2_arg) {{
  %state_buf = alloca [4096 x i8], align 16
  call void @llvm.memset.p0.i64(ptr %state_buf, i8 0, i64 4096, i1 false)
  %x0_ptr = getelementptr inbounds i8, ptr %state_buf, i64 {x0_off}
  %x1_ptr = getelementptr inbounds i8, ptr %state_buf, i64 {x1_off}
  %x2_ptr = getelementptr inbounds i8, ptr %state_buf, i64 {x2_off}
  %sp_ptr = getelementptr inbounds i8, ptr %state_buf, i64 {sp_off}
  store i64 %x0_arg, ptr %x0_ptr, align 8
  store i64 %x1_arg, ptr %x1_ptr, align 8
  store i64 %x2_arg, ptr %x2_ptr, align 8
  %sp_top = getelementptr inbounds i8, ptr %state_buf, i64 4096
  %sp_int = ptrtoint ptr %sp_top to i64
  store i64 %sp_int, ptr %sp_ptr, align 8
  %_ret_mem = call ptr @sub_{lift_addr_hex}(ptr %state_buf, i64 {lift_addr_dec}, ptr null)
  %ret_w = load i32, ptr %x0_ptr, align 4
  ret i32 %ret_w
}}
"#,
        lift_addr_hex = format!("{:x}", lift_addr),
        lift_addr_dec = lift_addr,
        x0_off = x0_off,
        x1_off = x1_off,
        x2_off = x2_off,
        sp_off = sp_off,
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
