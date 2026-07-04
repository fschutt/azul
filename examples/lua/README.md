# Azul — Lua / LuaJIT

Lua bindings for the [Azul](https://azul.rs) GUI framework via
LuaJIT's `ffi` module.

## Status

✅ **Full GUI E2E** — counter probe 5→8 via `AZ_DEBUG` verified
(macOS arm64). LuaJIT was the reference E2E implementation; every
other binding followed its pattern.

⚠️ **x86-64 (Linux/Windows)**: LuaJIT's aggregate-by-value NYI
(`NYI: cannot call this C function (yet)`) fires at `App.create`,
so the binding is effectively arm64-only today. See the
"Common errors" section of the Lua guide.

## Requirements

- LuaJIT 2.1+ (vanilla Lua doesn't have `ffi`)
- `libazul.dylib` (macOS) / `libazul.so` (Linux) in the working
  directory or on the loader path

## Build + Run

```sh
# macOS
DYLD_LIBRARY_PATH=. luajit hello-world.lua
# Linux
LD_LIBRARY_PATH=. luajit hello-world.lua
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

## Notes on past follow-ups

- **Memory-safety arc closed** (2026-05): `azul._consume`
  finalizer-disarm, Option/Result delete+clone, Vec iter clone.
- **`__eq` on `cdata == nil` SIGSEGV** — FIXED in current codegen:
  every generated `__eq` guards `type(a)`/`type(b)` `~= 'cdata'`
  before calling `_partialEq`.
- **`__tostring` leak** — FIXED in the emitter
  (`lang_lua/wrappers.rs`): the AzString returned by `toDbgString`
  is consumed via `AzString_delete` after `ffi.string()` copies the
  bytes. Lands in the vendored `azul.lua` on the next
  `azul-doc codegen all`.
- **`azul.Thread.create`** — the emitter now generates
  `error('ThreadCallback from Lua is unsupported; use the writeback pattern')`:
  LuaJIT has no runtime lock, so a Lua callback on a libazul worker
  thread would corrupt the VM (BINDING_STRATEGY_PER_LANGUAGE.md).
  Same guard on `MapWidget:dom_with_fetch` /
  `VideoWidget:dom_with_decoder`. Lands on the next regen.

## Files

- `hello-world.lua` — 48-line reference implementation (tracked).
- `azul-0.2.0-1.rockspec` — LuaRocks manifest (tracked).
- `azul.lua` — generated binding. **Not tracked in git** — download
  from `https://azul.rs/ui/release/0.2.0/azul.lua` or copy from
  `target/codegen/azul.lua` after `azul-doc codegen all`.
- `libazul.dylib` — prebuilt native library. **Not tracked in git** —
  download from the azul.rs release page or build via
  `cargo build -p azul-dll --release`.
