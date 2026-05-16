# Azul — OCaml

OCaml bindings for the [Azul](https://azul.rs) GUI framework via
`ctypes-foreign`.

## Status

✅ **Full GUI E2E** — counter probe 5→8 via `AZ_DEBUG` verified
(landed 2026-05-12).

## Requirements

- OCaml 4.14+ with `dune`
- `ctypes` + `ctypes-foreign` packages
- `libazul.dylib` in the working directory

## Build + Run

```sh
dune exec ./hello_world.exe
```

## What's idiomatic

- `Azul.String.to_string s` decodes the UTF-8 bytes into an OCaml string.
- `Azul.Update.refresh_dom` / `Azul.Update.do_nothing` — int constants
  in a module wrapper (snake_case because uppercase is reserved for
  constructors in OCaml).
- Per-Vec `<VecModule>.to_list` clones each element into an OCaml list
  (independent of the Vec's lifetime — closing the Vec is safe).
  Returns raw `<elem_ffi> Ctypes.structure list`; users wrap manually
  via `<ElemModule>.make_<snake>` if they want managed handles.
- Per-primitive-Vec `<VecModule>.to_array` bulk-copies the buffer into
  an OCaml-native type: `u8 → bytes`, `i8/u16/i16/u32/i32 → int array`,
  `u64/i64 → Ctypes-native array`, `f32/f64 → float array`.
- `Azul.az_option_<T>_intoSome r` / `Azul.az_result_<T>_intoOk r` /
  `_intoErr r` extract the payload as `<payload_ffi> Ctypes.structure
  option` (alignment-aware offset via `Ctypes.alignment`). Caveat:
  payload bytes are shared with the Option struct — clone via the
  per-class `<ElemModule>.clone` helper if you need an owned copy.
- Tag-byte accessors `is_some` / `is_none` / `is_ok` / `is_err` are
  emitted but currently umbrella-internal (not in the .mli).

## Files

- `hello_world.ml` — full GUI hello-world.
- `azul.ml` — generated Ctypes bindings.
- `dune-project` + `dune` — build config.
- `libazul.dylib` — prebuilt native library.

## Notes

The WCO smart factory (`A.3.7`) is deferred until a typed
tagged-union codegen rewrite happens — until then, users construct
the WCO via `Ctypes.setf` field-by-field on the `_default()` result.

## Recent updates (2026-05-15/16)

- **Memory-safety arc closed**: `azul_consume self_by_value` mechanism
  for owned-self C calls (rides on the JVM/CLR pass).
- **CC-6 `|>` pipeline hello-world** (commit `04948da25`): pure
  example rewrite; layout body composes top-down via local
  `with_*` helpers + `|>`.
- **V7 per-Vec `to_list` clone-via** (commit `bd8a7b71c`): 52
  wrapper-element Vec modules pick it up.
- **V7.2 per-Vec `to_array` primitive emit** (commit `84cfca5cb`):
  5 primitive Vec modules.
- **I.5.6 Option/Result payload extractor** (commit `d350b0fa9`):
  pure-Ctypes alignment-aware emit; no libazul-side
  `AzOption<T>_intoSome` export turned out to be needed.
