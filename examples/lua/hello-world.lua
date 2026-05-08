-- examples/lua/hello-world.lua
--
-- LuaJIT port of examples/c/hello-world.c.
--
-- Same data model (a `MyDataModel` struct with a uint32 counter),
-- same callback semantics (mouse click increments, layout renders).
--
-- ---------------------------------------------------------------------
-- LuaJIT FFI callback lifetime caveat (READ THIS):
--
-- `ffi.cast('AzCallback', luafn)` allocates a *trampoline* whose lifetime
-- is bound to the cdata returned by `ffi.cast`. If that cdata is
-- collected, the C side will jump into freed memory the next time the
-- callback fires. We therefore keep every casted callback in a
-- module-level Lua local (`live_callbacks`) so it stays referenced for
-- the entire process lifetime. This is the same workaround the LuaJIT
-- FFI manual recommends and what every long-lived FFI binding does.
-- ---------------------------------------------------------------------

local ffi  = require('ffi')
local azul = require('azul')
local C    = azul.C

-- ── Live-storage table for FFI callbacks (see caveat above) ───────────
local live_callbacks = {}

local function make_callback(c_typename, lua_fn)
    local cb = ffi.cast(c_typename, lua_fn)
    table.insert(live_callbacks, cb)        -- pin for process lifetime
    return cb
end

-- ── Data model ────────────────────────────────────────────────────────
--
-- We declare an FFI struct so the counter lives in C-allocated memory.
-- This is required because `AzRefAny_newC` copies `sizeof(MyDataModel)`
-- bytes from the supplied pointer into a heap-allocated RefAny.

ffi.cdef[[
    typedef struct { uint32_t counter; } MyDataModel;
]]

-- Destructor stub — MyDataModel owns no heap memory, so do nothing.
local model_destructor = make_callback(
    'AzRefAnyDestructorType',
    function(_ptr) end
)

-- A unique 64-bit RTTI id for this type. Any stable value works as long
-- as no other RefAny in the process uses the same id; collisions cause
-- spurious downcast successes. We pick a high value to avoid clashing
-- with built-in azul types.
local MY_DATA_MODEL_RTTI_ID = 0xA2010001ULL

local function upcast_model(model)
    -- SKIPPED: AzString_fromConstStr is a C macro and is therefore not
    -- visible through ffi.cdef. We build the type-name AzString via the
    -- regular byte-copy constructor instead.
    local name_bytes = 'MyDataModel'
    local name_str = C.AzString_copyFromBytes(
        ffi.cast('const uint8_t*', name_bytes), 0, #name_bytes)
    -- AzRefAny_newC(ptr, len, align, type_id, type_name, destructor)
    return C.AzRefAny_newC(
        ffi.cast('void*', model),
        ffi.sizeof('MyDataModel'),
        ffi.alignof('MyDataModel'),
        MY_DATA_MODEL_RTTI_ID,
        name_str,
        model_destructor)
end

local function downcast_mut(refany)
    -- Borrow a *mut MyDataModel from a RefAny. Returns nil if RTTI mismatch.
    if not C.AzRefAny_isType(refany, MY_DATA_MODEL_RTTI_ID) then
        return nil
    end
    return ffi.cast('MyDataModel*', C.AzRefAny_getDataPtr(refany))
end

-- ── Callback: increment the counter on click ──────────────────────────

local function on_click(data, info)
    local m = downcast_mut(data)
    if m == nil then
        return C.AzUpdate_DoNothing
    end
    m.counter = m.counter + 1
    return C.AzUpdate_RefreshDom
end
local on_click_cb = make_callback('AzCallbackType', on_click)

-- ── Layout callback ───────────────────────────────────────────────────

local function layout(data, info)
    local m = downcast_mut(data)
    if m == nil then
        return C.AzDom_createBody()
    end

    -- Counter label, wrapped in a div so it lays out as block.
    local buf  = tostring(m.counter)
    local txt  = C.AzString_copyFromBytes(
        ffi.cast('const uint8_t*', buf), 0, #buf)
    local label = C.AzDom_createText(txt)
    local label_wrapper = C.AzDom_createDiv()
    C.AzDom_addCssProperty(label_wrapper,
        C.AzCssPropertyWithConditions_simple(
            C.AzCssProperty_fontSize(C.AzStyleFontSize_px(32.0))))
    C.AzDom_addChild(label_wrapper, label)

    -- Button.
    local btn_text = C.AzString_copyFromBytes(
        ffi.cast('const uint8_t*', 'Increase counter'), 0,
        #'Increase counter')
    local button = C.AzButton_create(btn_text)
    C.AzButton_setButtonType(button, C.AzButtonType_Primary)
    local data_clone = C.AzRefAny_clone(data)
    C.AzButton_setOnClick(button, data_clone, on_click_cb)
    local button_dom = C.AzButton_dom(button)

    -- Body.
    local body = C.AzDom_createBody()
    C.AzDom_addChild(body, label_wrapper)
    C.AzDom_addChild(body, button_dom)

    return C.AzDom_style(body, C.AzCss_empty())
end
local layout_cb = make_callback('AzLayoutCallbackType', layout)

-- ── Main ──────────────────────────────────────────────────────────────

local model = ffi.new('MyDataModel', { counter = 5 })
local data  = upcast_model(model)

local window = C.AzWindowCreateOptions_create(layout_cb)
local title  = C.AzString_copyFromBytes(
    ffi.cast('const uint8_t*', 'Hello World'), 0, #'Hello World')
window.window_state.title = title
window.window_state.size.dimensions.width  = 400.0
window.window_state.size.dimensions.height = 300.0
window.window_state.flags.decorations =
    C.AzWindowDecorations_NoTitleAutoInject
window.window_state.flags.background_material =
    C.AzWindowBackgroundMaterial_Sidebar

local app = C.AzApp_create(data, C.AzAppConfig_create())
C.AzApp_run(app, window)
-- AzApp's __gc metamethod (registered by azul.lua) calls AzApp_delete
-- automatically when `app` is collected.
