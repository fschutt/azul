# Azul — Fortran (F2003+)

✓ **Full counter E2E passing** (2026-07-04). The historical
tagged-union codegen gap is fixed: `azul.f90` now emits every C
`repr(C, u8)` union as an ABI-opaque blob with the exact C size and
alignment (computed by `lang_fortran::layout`), so by-value struct
passing matches `azul.h` for all types (clang-verified, 1551/1551).
Unions are constructed/inspected via the C-API helper functions only —
Fortran has no native `union`, so field-level variant access is not
exposed.

## Status

- Full GUI counter example (`hello_world.f90`) passes the AZ_E2E
  headless scenario: initial DOM renders "5", three clicks make it "8".
- Callbacks go through the host-invoker dispatch layer
  (`azul_register_<kind>()` + `bind(C)` module procedures).

## Requirements

- GFortran (`brew install gcc` provides it on macOS)

## Build + Run

```sh
make
DYLD_LIBRARY_PATH=. ./hello_world
```

To run the headless counter E2E like CI does:

```sh
AZ_E2E=../../tests/e2e/hello_world_counter.json AZ_BACKEND=headless make run
```

## Files

- `hello_world.f90` — full-GUI counter example (layout + click callback).
- `azul.f90` — generated bindings (host-invoker layer included).
- `Makefile` — gfortran build (generated as `Makefile.fortran`).
- `libazul.dylib` — prebuilt native library.

## Notes

- Callbacks MUST live in a module (not as internal procedures) so
  `c_funloc()` needs no executable-stack trampoline.
- `owned = .false.` in the codegen-emitted consume helper disarms the
  F2003 finaliser for by-value C calls (mirrors Pascal's
  `FOwned := False` pattern).
