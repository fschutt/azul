# Azul — Fortran (F2003+)

✓ **Full counter E2E passing.** `azul.f90` is one module with two
layers: the raw `az_*` `bind(C)` interfaces mirroring `azul.h`
one-to-one, and an idiomatic wrapper layer on top (`dom_t`, `button_t`,
`app_t`, ...) that the example below uses exclusively.

## Status

- Full GUI counter example (`hello_world.f90`) passes the AZ_E2E
  headless scenario: initial DOM renders "5", three clicks make it "8".
- Callbacks are ordinary Fortran module functions matching a generated
  abstract interface (`layout_callback_iface`,
  `button_on_click_callback_iface`, ...). The `bind(C)` boundary lives
  inside `azul.f90`, not in your code.

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
- `azul.f90` — generated bindings (wrapper + host-invoker layers).
- `Makefile` — gfortran build (generated as `Makefile.fortran`).
- `libazul.dylib` — prebuilt native library.

## Notes

- `use azul` is the only `use` a program needs: the module re-exports
  the `iso_c_binding` entities (`c_ptr`, `c_loc`, `c_f_pointer`, ...)
  for anyone reaching down to the raw `az_*` layer.
- Wrapper types are named `<snake>_t` because Fortran folds case:
  `type(Dom) :: dom` would redeclare the type.
- There is no `final ::` finaliser by design — gfortran finalises a
  function result after the assignment that consumed it, which would
  `_delete` everything a factory ever returned. Call the explicit
  `delete` type-bound procedure when you own a value and want it gone;
  the `owned` flag makes a double `delete` a no-op.
- Callbacks and the model do NOT need `target, save`: `ref_any_create`
  copies the model into a binding-owned handle table, and the table
  frees it when libazul drops the last clone of the `RefAny`.
- Tagged unions (`AzOption*` / `AzResult*`) are ABI-opaque byte blobs —
  Fortran has no native `union`, so construct and inspect them through
  the C-API helper functions only, never through field access.
