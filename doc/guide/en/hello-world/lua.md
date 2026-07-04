---
slug: hello-world/lua
title: Hello World [Lua]
language: en
canonical_slug: hello-world/lua
audience: external
maturity: wip
guide_order: 18
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/lua/hello-world.lua
last_generated_rev: 39416ebc681c6423bfdefa94dc996f613184ea0b
generated_at: 2026-05-29T00:00:00Z
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
  - WindowCreateOptions
  - Update
---

# Hello World [Lua]

## Introduction

The Lua binding uses LuaJIT's `ffi` module to call the prebuilt `libazul` native
library. You work entirely through the idiomatic `azul.*` wrapper layer — no manual
`ffi.cast(...)` or raw `C.AzXxx_yyy(...)` calls. Callbacks route through libazul's
host-invoker plumbing, so LuaJIT never has to synthesize a struct-by-value
trampoline.

## Installation

You need **LuaJIT 2.1+** (vanilla Lua has no `ffi`) and the native `libazul` library.

There is no LuaRocks package yet - install manually:

1. Download the native library from the
   [release page](https://azul.rs/ui/release/$VERSION) (`libazul.dylib`
   / `libazul.so` / `azul.dll`).
2. Put the generated `azul.lua` next to `hello-world.lua`, or point
   `LUA_PATH` at it:

   ```sh
   wget https://azul.rs/ui/release/$VERSION/azul.lua
   ```

   (It is also produced by `cargo run --bin azul-doc -- codegen all` into
   `target/codegen/azul.lua` if you build from a checkout.)

## Simple "Counter" Example

```lua
local azul = require('azul')

-- Data model. azul.refany_create(value) wraps any Lua value into an AzRefAny;
-- azul.refany_get(refany) recovers it on the other side.
local model = { counter = 5 }

-- Click callback: a plain Lua function. The wrapper auto-routes it through the
-- host-invoker when you hand it to :set_on_click(...).
local function on_click(data, _info)
    local m = azul.refany_get(data)
    if m == nil then return azul.Update.DoNothing end
    m.counter = m.counter + 1
    return azul.Update.RefreshDom
end

-- Layout callback: f(data) -> Dom. Runs on startup and after RefreshDom.
local function layout(data, _info)
    local m = azul.refany_get(data)
    if m == nil then return azul.Dom.create_body() end

    -- add_* mutators return self (chain top-down); with_* consume self.
    local label = azul.Dom.create_div()
        :add_css_property(azul.CssPropertyWithConditions.simple(
            azul.CssProperty.font_size(azul.StyleFontSize.px(32.0))))
        :add_child(azul.Dom.create_text(tostring(m.counter)))

    local button_dom = azul.Button.create('Increase counter')
        :set_button_type(azul.ButtonType.Primary)
        :set_on_click(data:clone(), on_click)  -- :clone() bumps the refcount
        :dom()

    return azul.Dom.create_body()
        :add_child(label)
        :add_child(button_dom)
end

local data = azul.refany_create(model)

-- Fluent :with(opts) recursively assigns nested window-state fields and
-- auto-converts Lua strings to AzString.
local window = azul.WindowCreateOptions.create(layout):with({
    window_state = {
        title = 'Hello World',
        size = { dimensions = { width = 400.0, height = 300.0 } },
        flags = {
            decorations         = azul.WindowDecorations.NoTitleAutoInject,
            background_material = azul.WindowBackgroundMaterial.Sidebar,
        },
    },
})

local app = azul.App.create(data, azul.AppConfig.create())
app:run(window)
-- AzApp's __gc metamethod calls AzApp_delete automatically on collection.
```

Four things to notice.

- **`azul.refany_create` / `azul.refany_get`** — wrap any Lua value into a handle;
  an internal id-keyed table keeps it alive for the handle's lifetime. `data:clone()`
  bumps the refcount (thread-safe) so the click handler can recover it later.
- **Two builder flavours.** `add_*` / `set_*` mutate in place and return `self`
  (chain top-down); `with_*` consume `self` and return the new value. Both compose.
- **Plain Lua strings flow through** auto-string conversion — pass `'Increase counter'`
  and `tostring(m.counter)` directly; the wrapper converts to `AzString`.
- **Garbage collection is wired.** `AzApp`'s `__gc` metamethod calls `AzApp_delete`
  for you when `app` is collected.

## Build and run

```sh
# macOS
DYLD_LIBRARY_PATH=. luajit hello-world.lua
# linux
LD_LIBRARY_PATH=. luajit hello-world.lua
```

You should see the window pictured on the [hello-world landing page](../hello-world.md).
Click the button: the counter increments and the layout callback re-runs.

## Common errors

- **`module 'azul' not found`** — `azul.lua` is not on `LUA_PATH`. Run LuaJIT from the
  directory that contains it, or set `LUA_PATH="./?.lua;$LUA_PATH"`.
- **`cannot open libazul`** — the native library isn't on `DYLD_LIBRARY_PATH` /
  `LD_LIBRARY_PATH`.
- **`attempt to index a nil value` from `ffi`** — you are on vanilla Lua, not LuaJIT.
  The `ffi` module ships only with LuaJIT.
- **`NYI: cannot call this C function (yet)` at `App.create`** — LuaJIT's `ffi`
  cannot call a C function that takes an aggregate **by value** on some ABIs.
  On **x86-64 (SysV)** `App.create(.., AppConfig)` hits this; it works on
  **arm64/macOS** (the struct is passed differently). It is a LuaJIT limitation,
  not a version issue (a current LuaJIT 2.1 still NYIs) — there is no Lua-side
  workaround short of a by-pointer C-ABI, so the E2E board marks Lua `⊘ SKIP` on
  x86-64.
- **Counter does not advance** — `on_click` returned `azul.Update.DoNothing`.

## Coming Up Next

- [Application Architecture](../architecture.md) — architecting a larger Azul application
- [Document Object Model](../dom.md) — the Dom tree: node types, hierarchy, and CSS
- [Hello World [Ruby]](ruby.md)
