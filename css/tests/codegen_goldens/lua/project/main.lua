-- Copy target/codegen/azul.lua and libazul next to this file, then: luajit main.lua
local styles = require('styles')

local style_btn = styles.style_btn()
print(('style_btn: %d properties'):format(tonumber(style_btn.len)))
