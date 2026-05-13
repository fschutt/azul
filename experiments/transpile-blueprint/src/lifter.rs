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

// ── RemillLifter (feature-gated) ────────────────────────────────────────

#[cfg(feature = "remill")]
pub struct RemillLifter;

#[cfg(feature = "remill")]
impl LlvmLifter for RemillLifter {
    fn name(&self) -> &str {
        "RemillLifter (remill v6 via cxx)"
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
            crate::ffi::ffi::lift_bytes_to_llvm_ir(arch.tag(), bytes, base_addr)
                .map_err(|e| format!("remill shim: {e}"))?;
        if ir.is_empty() {
            return Err("remill returned empty IR (lift failed)".into());
        }
        Ok(LiftedIr {
            ir,
            export_symbol: export_symbol.to_string(),
        })
    }
}
