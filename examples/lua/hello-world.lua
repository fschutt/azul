-- LD_LIBRARY_PATH=. luajit hello-world.lua

local azul = require('azul')

local model = { counter = 5 }

local function on_click(data, _info)
    local m = azul.refany_get(data)
    if m == nil then return azul.Update.DoNothing end
    m.counter = m.counter + 1
    return azul.Update.RefreshDom
end

local function layout(data, _info)
    local m = azul.refany_get(data)
    if m == nil then return azul.Dom.create_body() end

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

local data   = azul.refany_create(model)

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
