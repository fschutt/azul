# Azul — Fortran (F2003+)

⊘ **Codegen rewrite needed.** Fortran's `emit_tagged_union` produces
opaque `(tag: c_int + payload: c_ptr)` 12-byte structs instead of
the inline `#[repr(C, u8)]` union the C ABI uses. Any function that
takes/returns `AzOption<T>` / `AzResult<T,E>` by value gets corrupted
struct layout. See `memory/fortran_codegen_2026_05_13.md`.

## Status

- Smoke test (AzString round-trip + refany_create) verified.
- Full GUI: not reachable without the tagged-union codegen rewrite.

## Requirements

- GFortran (`brew install gcc` provides it on macOS)

## Build + Run (smoke only)

```sh
make
DYLD_LIBRARY_PATH=. ./hello_world
```

## Files

- `hello_world.f90` — smoke test.
- `azul.f90` — generated bindings.
- `Makefile` — gfortran build.
- `libazul.dylib` — prebuilt native library.

## Recent updates (2026-05-15/16)

- **R11 consume mechanism** (commit `7f39e0c03`): `owned = .false.`
  in the codegen-emitted consume helper disarms the F2003 finaliser
  for by-value C calls. Mirrors the Pascal `FOwned := False` pattern.
