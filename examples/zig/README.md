# Azul — Zig

Zig bindings for the [Azul](https://azul.rs) GUI framework via
`@cImport`.

## Status

✅ **Full GUI E2E** — counter probe 5→8 verified.

## Requirements

- Zig 0.11+
- `libazul.dylib` available at link time

## Build + Run

```sh
zig build-exe hello-world.zig -lc -lazul -L. -rpath . -framework Foundation
DYLD_LIBRARY_PATH=. ./hello-world
```

## What's idiomatic

Zig's `@cImport` produces typed `c.AzString` / `c.AzDom` etc.
directly from the generated `azul.h`. User code calls C-ABI
functions through `c.<fn>` with no wrapper layer. Callback
functions use `callconv(.c)` to match the C calling convention.

```zig
fn my_layout(data: c.AzRefAny, info: c.AzLayoutCallbackInfo) callconv(.c) c.AzDom {
    // ...
}
```

The host-invoker pattern isn't needed — Zig's `callconv(.c)` produces
real C function pointers from comptime-known functions, so the framework
can call back into Zig directly.

`AzRefAny_newC` + `AzRefAny_isType` + `AzRefAny_getDataPtr` handle
the host-managed data lifecycle without a side handle table.

## Files

- `hello-world.zig` — 133-line reference implementation.
- `libazul.dylib` — prebuilt native library.

## Recent updates (2026-05-15/16)

- **R10 consume mechanism** (commit `dbc7d82b9`): `consumed: bool`
  field on each wrapper struct; consume helper sets it to true to
  skip the `Az<X>_delete` call in `deinit()`. Closes the double-free
  risk from consuming-self method bodies.
