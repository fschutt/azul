-- Copy target/codegen/azul.lua and libazul next to this file, then: luajit main.lua
local styles = require('styles')

local render_ui = styles.render_ui()
print(('render_ui: %d properties'):format(tonumber(render_ui.len)))
