# Review: layout/src/file.rs

## Summary
- Lines: 938
- Public functions: 22 (free functions) + ~40 (FilePath methods)
- Public structs/enums: 5 (FileError, FileErrorKind, FileType, FileMetadata, DirEntry, FilePath)
- Findings: 1 high, 1 medium, 1 low

## Findings

### [HIGH] Dead Code — FilePath, free functions, and most types are unused outside this file
- **Location**: Entire file
- **Details**: `FilePath` is only referenced in `layout/src/lib.rs` (re-export) and `layout/src/file.rs` itself. No code anywhere in the codebase actually constructs or uses a `FilePath`. Similarly, the free functions (`file_read`, `file_write`, etc.) are re-exported from `lib.rs` but have zero call sites outside `file.rs`. The FFI result types (`ResultVoidFileError`, `ResultU8VecFileError`, etc.) are also only referenced within this file and `lib.rs`.
- **Evidence**: `grep -r "FilePath" --include="*.rs"` returns only `layout/src/file.rs` and `layout/src/lib.rs`. `grep -r "file_read_string\|file_append\|path_parent\|path_file_name\|path_extension\|path_canonicalize" --include="*.rs"` returns only `layout/src/file.rs`.
- **Recommendation**: Either wire these into the C API / DLL layer (the stated purpose of the module) or remove the dead code. Note `layout/src/desktop/file.rs` already provides a `File` struct with overlapping read/write functionality that IS wired into the DLL.

### [MEDIUM] Duplicated Functionality — Overlaps with `layout/src/desktop/file.rs`
- **Location**: `file.rs` (entire file) vs `layout/src/desktop/file.rs`
- **Details**: `layout/src/desktop/file.rs` provides a `File` struct with `read_to_string`, `read_to_bytes`, `write_string`, `write_bytes`, and `open`/`create` methods. `layout/src/file.rs` provides free functions `file_read`, `file_read_string`, `file_write`, `file_write_string` that do the same thing. The desktop `File` struct is wired into the DLL layer (`dll/src/desktop/mod.rs:57: pub use azul_layout::desktop::file::*;`) while this file's types are not.
- **Evidence**: `layout/src/desktop/file.rs` has `read_to_string`, `read_to_bytes`, `write_string`, `write_bytes`. This file has `file_read`, `file_read_string`, `file_write`, `file_write_string`. Both wrap `std::fs`.
- **Recommendation**: Consolidate. Either extend the desktop `File` to cover the additional operations (append, copy, rename, delete, metadata, directory ops) or have the desktop `File` delegate to these free functions. Remove the duplicate.

### [LOW] Potential compatibility concern — `ErrorKind::IsADirectory` / `DirectoryNotEmpty`
- **Location**: `file.rs:68-69`
- **Details**: `ErrorKind::IsADirectory` and `ErrorKind::DirectoryNotEmpty` were stabilized in Rust 1.83.0 (Nov 2024). If the project needs to support older Rust toolchains, these will fail to compile. The `Cargo.toml` does not specify a `rust-version`.
- **Recommendation**: If supporting Rust < 1.83 is needed, map these variants in the `_` arm instead. Otherwise, consider adding `rust-version = "1.83"` to `Cargo.toml` to document the requirement.

## System Documentation
- System identified: File system / C API utilities
- Existing doc: none (no guide for file system operations)
- Doc needed: A brief guide on how file I/O is exposed to C/C++/Python consumers, covering both the `desktop::file::File` struct and the free functions in `file.rs`, and how they relate to each other. Could be part of a broader "C API" or "FFI" guide.
