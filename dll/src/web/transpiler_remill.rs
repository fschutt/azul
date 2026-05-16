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
        let ir_path = self.scratch_dir.join(format!("{}.ll", sanitize_filename(fn_name)));
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
                ir_path.to_str().expect("scratch path is utf-8"),
            ],
            fn_name,
        )?;

        // llc → wasm32 object
        let obj_path = self.scratch_dir.join(format!("{}.o", sanitize_filename(fn_name)));
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
            fn_name,
        )?;

        // wasm-ld → final module
        let wasm_path = self.scratch_dir.join(format!("{}.wasm", sanitize_filename(fn_name)));
        let export_arg = format!("--export={}", remill_export_symbol(lift_addr));
        run_tool(
            tools.wasm_ld,
            &[
                "--no-entry",
                &export_arg,
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
            exports: vec![remill_export_symbol(lift_addr)],
            // TODO(WB1.3): scan the lifted IR for external `call`s and
            // surface them as imports from azul-mini.wasm. For now,
            // leave empty so the caller treats the module as
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
        self.remill_lift.is_some() && self.llc.is_some() && self.wasm_ld.is_some()
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
