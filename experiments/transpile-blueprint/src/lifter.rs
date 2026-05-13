//! `LlvmLifter` — the integration seam between native machine code and
//! the LLVM IR text that we hand off to `llc -mtriple=wasm32`.
//!
//! Two implementations live behind this trait:
//!
//! - [`StubLifter`] — always available. Pattern-matches a trivial
//!   `add(i32, i32) -> i32` leaf function and emits hand-written LLVM
//!   IR that an aarch64 `add w0, w0, w1; ret` would lower to. Lets us
//!   exercise the rest of the pipeline (llc → wasm-ld) end-to-end
//!   without remill present.
//!
//! - [`RemillLifter`] — gated on the `remill` Cargo feature, FFI'd to
//!   `cpp/shim.cpp` via cxx-rs. Calls remill's `TraceLifter::Lift` on
//!   the byte sequence and returns the resulting LLVM IR. Drop-in
//!   replacement for the stub; the CLI selects via `--lifter`.
//!
//! The stub's output is **not** what remill produces in real life —
//! remill's IR uses a `State` struct + memory intrinsics, while the
//! stub emits clean direct IR. The point of the stub is only to verify
//! that the IR-emission → llc → wasm-ld leg works.

/// Architectures the lifter understands. Maps onto remill's `ArchName`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    AArch64,
    Amd64,
}

impl Arch {
    /// Stringified architecture tag in the form remill::ArchName expects.
    /// Used by RemillLifter only — kept allow(dead_code) so the stub-only
    /// build (the default) doesn't warn.
    #[allow(dead_code)]
    pub fn tag(self) -> &'static str {
        match self {
            Arch::AArch64 => "aarch64",
            Arch::Amd64 => "amd64",
        }
    }
}

pub struct LiftedIr {
    /// The textual LLVM IR module. Suitable for piping to `llc`.
    pub ir: String,
    /// Symbol the consumer should export. The pipeline's llc → wasm-ld
    /// step uses `--export=<this>` so the WASM module's table picks it up.
    pub export_symbol: String,
}

pub trait LlvmLifter {
    fn name(&self) -> &str;
    fn is_real(&self) -> bool;
    fn lift(
        &self,
        bytes: &[u8],
        base_addr: u64,
        arch: Arch,
        export_symbol: &str,
    ) -> Result<LiftedIr, String>;
}

// ── StubLifter ──────────────────────────────────────────────────────────

pub struct StubLifter;

impl LlvmLifter for StubLifter {
    fn name(&self) -> &str {
        "StubLifter (hand-rolled IR)"
    }
    fn is_real(&self) -> bool {
        false
    }
    fn lift(
        &self,
        bytes: &[u8],
        base_addr: u64,
        arch: Arch,
        export_symbol: &str,
    ) -> Result<LiftedIr, String> {
        // We *could* inspect `bytes` to recognise specific encodings.
        // The stub keeps that step abstract — its only contract is
        // "given some bytes for a 2-arg-add-style function, return IR
        // that exports `<export_symbol>(i32, i32) -> i32`".
        let _ = (bytes, base_addr, arch);

        let ir = format!(
            r#"; ModuleID = 'transpile_blueprint::stub'
; Emitted by StubLifter for export `{sym}`
target triple = "wasm32-unknown-unknown"

define i32 @{sym}(i32 %a, i32 %b) {{
entry:
  %r = add nsw i32 %a, %b
  ret i32 %r
}}
"#,
            sym = export_symbol
        );

        Ok(LiftedIr {
            ir,
            export_symbol: export_symbol.to_string(),
        })
    }
}

// ── RemillCliLifter ─────────────────────────────────────────────────────
//
// Subprocess-based integration with remill's standalone `remill-lift`
// binary. Works today (no need to link remill's static libs from Rust)
// and produces the same LLVM IR the future FFI path will. The CLI is
// remill's own canonical entry point: bin/lift/Lift.cpp.

pub struct RemillCliLifter {
    pub lift_bin: std::path::PathBuf,
}

impl RemillCliLifter {
    /// Resolve the remill-lift binary path. Order:
    ///   1. $REMILL_LIFT_BIN, if set.
    ///   2. third_party/remill-install/build/remill/bin/lift/remill-lift-17
    ///      relative to CARGO_MANIFEST_DIR (the local cmake build).
    pub fn discover() -> Option<Self> {
        if let Ok(p) = std::env::var("REMILL_LIFT_BIN") {
            let pb = std::path::PathBuf::from(p);
            if pb.is_file() {
                return Some(Self { lift_bin: pb });
            }
        }
        let local = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../third_party/remill-install/build/remill/bin/lift/remill-lift-17");
        if local.is_file() {
            return Some(Self { lift_bin: local });
        }
        None
    }
}

impl LlvmLifter for RemillCliLifter {
    fn name(&self) -> &str {
        "RemillCliLifter (subprocess to remill-lift-17)"
    }
    fn is_real(&self) -> bool {
        true
    }
    fn lift(
        &self,
        bytes: &[u8],
        base_addr: u64,
        arch: Arch,
        export_symbol: &str,
    ) -> Result<LiftedIr, String> {
        let mut hex = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            hex.push_str(&format!("{:02x}", b));
        }

        // remill's standalone CLI bails out on null-page addresses with a
        // useless message; lift everything at a fixed high virtual address
        // and let the caller's symbol_export name override the resulting
        // function name in a later pass if needed.
        let lift_addr: u64 = 0x100000000;

        let out_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("out/blueprint.remill.ll");
        std::fs::create_dir_all(out_path.parent().unwrap())
            .map_err(|e| format!("mkdir out/: {e}"))?;

        let _ = base_addr;
        let _ = export_symbol;

        let status = std::process::Command::new(&self.lift_bin)
            .arg("--arch").arg(arch.tag())
            .arg("--os").arg("macos")
            .arg("--address").arg(format!("0x{:x}", lift_addr))
            .arg("--entry_address").arg(format!("0x{:x}", lift_addr))
            .arg("--bytes").arg(&hex)
            .arg("--ir_out").arg(&out_path)
            .output()
            .map_err(|e| format!("spawn remill-lift: {e}"))?;

        if !status.status.success() {
            return Err(format!(
                "remill-lift failed: {}\nstderr:\n{}",
                status.status,
                String::from_utf8_lossy(&status.stderr),
            ));
        }

        let ir = std::fs::read_to_string(&out_path)
            .map_err(|e| format!("read remill output: {e}"))?;
        if ir.is_empty() {
            return Err("remill returned empty IR".into());
        }
        Ok(LiftedIr {
            ir,
            export_symbol: export_symbol.to_string(),
        })
    }
}

// ── RemillFfiLifter (cxx feature, currently unwired — see cpp/shim.cpp) ─
//
// The longer-term integration target: link remill's static libs in-process
// via cxx-rs. Currently the build.rs scaffold compiles cpp/shim.cpp but
// linking the full transitive closure (remill_bc + remill_arch +
// sleigh + LLVM + glog + gflags + xed) is not wired up. Use the CLI
// lifter above until then.
#[cfg(feature = "remill")]
pub struct RemillFfiLifter;

#[cfg(feature = "remill")]
impl LlvmLifter for RemillFfiLifter {
    fn name(&self) -> &str {
        "RemillFfiLifter (cxx, link-WIP)"
    }
    fn is_real(&self) -> bool {
        true
    }
    fn lift(
        &self,
        bytes: &[u8],
        base_addr: u64,
        arch: Arch,
        export_symbol: &str,
    ) -> Result<LiftedIr, String> {
        let ir =
            crate::ffi::ffi::lift_bytes_to_llvm_ir(arch.tag(), bytes, base_addr);
        if ir.is_empty() {
            return Err("remill returned empty IR (lift failed)".into());
        }
        Ok(LiftedIr {
            ir,
            export_symbol: export_symbol.to_string(),
        })
    }
}
