-- Lua: nil where an Option is asked for, nil or the value back from fields
-- and downcasts, a raised error for an Err (inside a callback the binding
-- logs it and the callback does nothing). Run by
-- scripts/native_optionals_smoke.sh.
local azul = require('azul')

-- The guide's file-dialog example, as written there (hello-world/lua.md).
local function on_pick_folder(model, _info, result)
    local picked = azul.FileOpenResult.downcast(result)
    local path = picked and picked:get_path()
    if path == nil then return azul.Update.DoNothing end -- not a folder pick, or cancelled
    model.folder = path:get_inner()
    return azul.Update.RefreshDom
end

local function on_choose_folder(model, _info)
    azul.FileDialog.open_directory('Choose a folder', nil, model, on_pick_folder)
    return azul.Update.DoNothing
end

local function on_open_link(model, _info)
    azul.Url.parse(model.link) -- an Err raises: logged, the callback does nothing
    return azul.Update.RefreshDom
end

local function check(ok, what)
    if not ok then
        io.stderr:write('FAIL: ' .. what .. '\n')
        os.exit(1)
    end
end

-- A field: nil in, nil out; a value in, the value out.
local icons = azul.IconVariants.createDefault()
check(icons:get_light() == nil, 'a default IconVariants has no light icon')
icons:set_light('light.png')
check(icons:get_light() == 'light.png', 'Some in, Some out')
icons:set_light(nil)
check(icons:get_light() == nil, 'None in, None out')

-- A result: Ok is the value, Err a raised error.
check(azul.Url.parse('https://azul.rs/guide') ~= nil, 'Url.parse Ok')
check(not pcall(azul.Url.parse, 'no scheme'), 'Url.parse Err raises')

azul.Button.create('Open'):with_on_click({ folder = '', link = 'no scheme' }, on_open_link)
check(type(on_choose_folder) == 'function', 'the guide callbacks exist')
print('lua native optionals: ok')
