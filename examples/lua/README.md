# Azul — Lua / LuaJIT

Lua bindings for the [Azul](https://azul.rs) GUI framework via
LuaJIT's `ffi` module.

## Status

✅ **Full GUI E2E** — counter probe 5→8 via `AZ_DEBUG` verified.
LuaJIT was the reference E2E implementation; every other binding
followed its pattern.

## Requirements

- LuaJIT 2.1+ (vanilla Lua doesn't have `ffi`)
- `libazul.dylib` in the working directory

## Build + Run

```sh
DYLD_LIBRARY_PATH=. luajit hello-world.lua
```

## What's idiomatic

- `azul.WindowCreateOptions.create(layout)` smart factory — accepts
  a Lua function. The codegen auto-registers via
  `azul._register_callback('LayoutCallback', fn)` and splices
  directly into `wco.window_state.layout_callback`.
- `btn:on_click(data, fn)` — wraps `data` via `azul.refany_create`,
  delegates to `:with_on_click` which auto-registers.
- `azStr:to_lua_string()` — `ffi.string(self.vec.ptr, self.vec.len)`.
- `option:to_opt()` / `:is_some()` / `:is_none()` and
  `result:unwrap()` / `:is_ok()` / `:is_err()` via the `ffi.metatype`
  attached to each cdef union.
- Enum constants: `azul.Update.RefreshDom`, `azul.ButtonType.Primary`.
- Fluent `:with(opts)` builder on every struct wrapper: recursively
  assigns nested cdata fields from a Lua table, auto-converting
  Lua strings to AzString. Returns self for chaining.
- Chainable void mutators: `add_child` / `set_button_type` etc.
  now return self (CC-6), so `body:add_child(label):add_child(button)`
  composes top-down.

## Recent updates (2026-05-15/16)

- **Memory-safety arc closed**: `azul._consume` finalizer-disarm
  (commit `2bb53b89` ish), Option/Result delete+clone (`654b8cbd8`),
  Vec iter clone (`bb06ba101`).
- **CC-4 `:with(opts)` builder** (commit `9125d6c53`).
  Surfaced a pre-existing `__eq` SIGSEGV bug on `cdata == nil` —
  worked around in `_apply_opts`; root cause flagged for follow-up
  (`__eq` should guard `type(b) ~= 'cdata'` before passing to
  `_partialEq`).
- **CC-6 void-mutator chaining** (commit `76b804213`): every
  void-returning MethodMut now returns `self` so `add_*` / `set_*`
  chain. `with_*` (consume-self) still returns the new instance.

## Files

- `hello-world.lua` — 93-line reference implementation.
- `azul.lua` — generated bindings.
- `azul-1-1.rockspec` — LuaRocks manifest.
- `libazul.dylib` — prebuilt native library.
