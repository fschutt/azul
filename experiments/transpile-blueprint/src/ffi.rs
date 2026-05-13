//! cxx-rs bridge to `cpp/shim.cpp`.
//!
//! Only compiled when the `remill` feature is set. With the feature off
//! the [`crate::lifter::StubLifter`] is the only path through and this
//! file is excluded from the build via the cfg below.

#![cfg(feature = "remill")]

#[cxx::bridge(namespace = "transpile_blueprint")]
pub mod ffi {
    unsafe extern "C++" {
        include!("transpile-blueprint/cpp/shim.h");

        /// Lift `bytes` (placed at virtual address `base_addr`) for
        /// architecture `arch_tag` to textual LLVM IR.
        ///
        /// Architectures map onto `remill::ArchName`:
        ///   - `"aarch64"` → kArchAArch64LittleEndian
        ///   - `"amd64"`   → kArchAMD64
        ///
        /// Returns the empty string on failure; the C++ side logs to
        /// stderr.
        fn lift_bytes_to_llvm_ir(
            arch_tag: &str,
            bytes: &[u8],
            base_addr: u64,
        ) -> String;
    }
}
