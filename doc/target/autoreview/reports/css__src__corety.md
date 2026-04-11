# Review: css/src/corety.rs

## Summary
- Lines: 620
- Public functions: ~20 (on AzString, Void, LayoutDebugMessage, U8Vec)
- Public structs/enums: 3 hand-written (Void, LayoutDebugMessageType, LayoutDebugMessage) + many macro-generated (AzString, U8Vec, U16Vec, F32Vec, U32Vec, StringVec, Option*/Vec* types)
- Findings: 4 high, 1 medium (1 fixed), 0 low

## Findings

### [HIGH] Dead Code — `Void` struct has zero external call sites
- **Location**: `corety.rs:30-59`
- **Details**: `Void::new()` and `Void::default()` are only referenced in `api.json`, `layout/src/file.rs`, and `layout/src/thread.rs`. The struct itself is only defined in this file. The `From<()>` and `From<Void>` impls have no callers outside this file.
- **Evidence**: `grep "struct Void"` → only `css/src/corety.rs`. `grep "Void::new|Void::default"` → `api.json`, `css/src/corety.rs`, `layout/src/file.rs`, `layout/src/thread.rs`. Usage is minimal; may be vestigial.
- **Recommendation**: Verify whether `Void` is still needed for the FFI API. If only used in a couple of places, consider removing and using a simple `u8` directly.

### [HIGH] Dead Code — `AzString::to_c_str()` unused outside definition
- **Location**: `corety.rs:317-323`
- **Details**: `to_c_str()` is only found in `api.json` and `css/src/corety.rs`. No Rust call sites.
- **Evidence**: `grep "to_c_str"` → only `api.json` and `css/src/corety.rs`.
- **Recommendation**: Remove or mark with `#[cfg(feature = "ffi")]` if only needed for generated bindings.

### [HIGH] Dead Code — `AzString::from_c_str()` unused outside definition
- **Location**: `corety.rs:231-237`
- **Details**: Only found in `api.json` and `css/src/corety.rs`. No Rust call sites.
- **Evidence**: `grep "from_c_str"` → only `api.json` and `css/src/corety.rs`.
- **Recommendation**: Same as `to_c_str()`.

### [HIGH] Dead Code — `AzString::copy_from_bytes()` unused outside definition
- **Location**: `corety.rs:243-247`
- **Details**: `AzString::copy_from_bytes` (the wrapper) is only found in `api.json` and this file. The underlying `U8Vec::copy_from_bytes` is also only in `api.json` and this file (plus `scripts/ARCH_TODO.md`).
- **Evidence**: `grep "AzString::copy_from_bytes"` → only `api.json`.
- **Recommendation**: Remove or gate behind FFI feature if only for generated bindings.

### [MEDIUM] Dead Code — `AzString::from_utf16_le`, `from_utf16_be`, `from_utf8_lossy`, `from_utf8` unused
- **Location**: `corety.rs:336-423`
- **Details**: These four `unsafe` methods on `AzString` appear only in `api.json` and the definition file. No Rust call sites in the codebase.
- **Evidence**: `grep "AzString::from_utf16_le|AzString::from_utf16_be|AzString::from_utf8_lossy|AzString::from_utf8"` → only `api.json`.
- **Recommendation**: These are likely FFI entry points generated from `api.json`. Consider gating behind `#[cfg(feature = "ffi")]` or documenting that they exist solely for C/Python bindings.

## System Documentation
- System identified: yes — Core FFI type system (fundamental types used across crate boundaries)
- Existing doc: none (no specific guide for the FFI type system)
- Doc needed: A guide explaining the FFI-safe type system: `AzString`, `U8Vec`, destructor patterns, the `impl_vec!`/`impl_option!` macro system, and how these types are used across the C/Python/Rust APIs. This could be part of a broader "architecture.md" update or a standalone "ffi-types.md" guide.
