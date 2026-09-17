local azul = require('azul')

local model = { counter = 5 }

local function on_click(data, _info)
    data.counter = data.counter + 1
    return azul.Update.RefreshDom
end

local function layout(data, _info)
    local label = azul.Dom.create_p_with_text(tostring(data.counter))
        :with_css('font-size: 32px; margin: 0;')

    local button_dom = azul.Button.create('Increase counter')
        :set_button_type(azul.ButtonType.Primary)
        :with_on_click(data, on_click)
        :dom()

    return azul.Dom.create_body()
        :add_child(label)
        :add_child(button_dom)
end

local window = azul.WindowCreateOptions.create(layout):with({
    window_state = {
        title = 'Hello World',
        size = { dimensions = { width = 400.0, height = 300.0 } },
    },
})

local app = azul.App.create(model, azul.AppConfig.create())
app:run(window)
