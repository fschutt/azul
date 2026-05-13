#pragma once
//
// Minimal C++ shim exposed to Rust via cxx-rs.
//
// One entry point: lift_bytes_to_llvm_ir(arch_tag, bytes, base_addr).
// Returns the textual LLVM IR for the lifted trace as a std::string.
//
// The function is built only when the `remill` Cargo feature is enabled.
// Without the feature, the Rust side never calls into here — StubLifter
// handles the path with hand-written IR.

#include <cstdint>
#include <string>
#include "rust/cxx.h"

namespace transpile_blueprint {

// Lift `bytes` (interpreted as starting at virtual address `base_addr`,
// in architecture `arch_tag`) to textual LLVM IR. Returns an empty
// string on failure; the caller logs diagnostics from stderr.
//
// `arch_tag` values that match remill::ArchName:
//   "aarch64"  → kArchAArch64LittleEndian
//   "amd64"    → kArchAMD64
//   "x86"      → kArchX86
//
// Returns rust::String so the IR text crosses the FFI boundary as an
// owned Rust String; cxx handles the move.
rust::String lift_bytes_to_llvm_ir(
    rust::Str arch_tag,
    rust::Slice<const std::uint8_t> bytes,
    std::uint64_t base_addr);

}  // namespace transpile_blueprint
