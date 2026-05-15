-- examples/lua/hello-world.lua
--
-- LuaJIT port of examples/c/hello-world.c.
--
-- Same data model (a counter), same behaviour (mouse click increments,
-- layout rebuilds the DOM). Uses the idiomatic `azul.*` wrapper layer
-- only — no manual `ffi.cast(...)` or raw `C.AzXxx_yyy(...)` calls.
-- Callbacks go through libazul's host-invoker plumbing
-- (`AzCallback_createFromHostHandle`, `AzApp_setCallbackInvoker`)
-- so we never need LuaJIT to synthesize a struct-by-value trampoline.
--
-- Run with:
--     LD_LIBRARY_PATH=path/to/libazul luajit hello-world.lua
--     # macOS:
--     DYLD_LIBRARY_PATH=path/to/libazul luajit hello-world.lua
--
-- Requires LuaJIT 2.0+ (the bundled `ffi` module is not part of standard
-- Lua). The generated `azul.lua` lives in `target/codegen/azul.lua` after
-- `cargo run --bin azul-doc -- codegen all`. Either copy it next to
-- `hello-world.lua` or set `LUA_PATH` to find it.

local azul = require('azul')

-- ── Data model ────────────────────────────────────────────────────────
--
-- `azul.refany_create(value)` wraps any Lua value into an AzRefAny. The
-- value is held alive for the RefAny's lifetime by an internal id-keyed
-- table; `azul.refany_get(refany)` recovers it on the other side. The
-- destructor that fires when the last RefAny clone drops calls back
-- through `AzApp_setHostHandleReleaser` to clear the entry.

local model = { counter = 5 }

-- ── Callback: button click ────────────────────────────────────────────
--
-- Plain Lua function. The wrapper layer auto-routes this through
-- `azul._register_callback('Callback', on_click)` when we hand it to
-- `button:set_on_click(...)`, which uses libazul's static thunk to
-- dispatch back into Lua via the registered `AzCallbackInvoker`.

local function on_click(data, _info)
    local m = azul.refany_get(data)
    if m == nil then return azul.Update.DoNothing end
    m.counter = m.counter + 1
    return azul.Update.RefreshDom
end

-- ── Layout callback ───────────────────────────────────────────────────

local function layout(data, _info)
    local m = azul.refany_get(data)
    if m == nil then return azul.Dom.create_body() end

    -- CC-6: mutators (`add_*` / `set_*`) now return self so the wrapper
    -- chain reads top-down. Plain Lua strings flow through auto-string
    -- conversion. `with_*` consumes self (and returns the new instance),
    -- mirroring the by-value Rust builder; `add_*` mutates in place and
    -- returns self for syntactic chaining only — both compose cleanly.
    local label_wrapper = azul.Dom.create_div()
        :add_css_property(azul.CssPropertyWithConditions.simple(
            azul.CssProperty.font_size(azul.StyleFontSize.px(32.0))))
        :add_child(azul.Dom.create_text(tostring(m.counter)))

    local button_dom = azul.Button.create('Increase counter')
        :set_button_type(azul.ButtonType.Primary)
        :set_on_click(data:clone(), on_click)
        :dom()

    return azul.Dom.create_body()
        :add_child(label_wrapper)
        :add_child(button_dom)
end

-- ── Main ──────────────────────────────────────────────────────────────

local data   = azul.refany_create(model)

-- Fluent `:with(opts)` builder: recursively assigns nested window-state
-- fields, auto-converting Lua strings to AzString. Replaces the prior
-- `window.window_state.field = ...` drilling. NoTitleAutoInject lets
-- the OS draw close/min/max buttons while the framework auto-injects
-- a draggable titlebar.
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
-- AzApp's __gc metamethod (registered by azul.lua) calls AzApp_delete
-- automatically when `app` is collected.
