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
- Other auto-conversion helpers are partial because OCaml's
  tagged-union emission is opaque-blob (libffi-marshal constraint):
  - `Azul.az_option_<T>_is_some r` / `_is_none r` — tag-byte
    accessors via `Ctypes.coerce` to a uint8_t pointer.
  - `Azul.az_result_<T>_is_ok r` / `_is_err r` — same.
  - Payload extraction for AzOption/AzResult/AzVec is NOT exposed
    — would need per-variant typed Ctypes structs (separate codegen
    rewrite).

## Files

- `hello_world.ml` — full GUI hello-world.
- `azul.ml` — generated Ctypes bindings.
- `dune-project` + `dune` — build config.
- `libazul.dylib` — prebuilt native library.

## Notes

The WCO smart factory (`A.3.7`) is deferred until a typed
tagged-union codegen rewrite happens — until then, users construct
the WCO via `Ctypes.setf` field-by-field on the `_default()` result.
