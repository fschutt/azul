-- Memory test for the azul Lua (LuaJIT ffi) binding. See tests/memtest/README.md.
--
-- The harness measures RSS/gdb externally; this file just exercises the
-- create/consume/DROP paths in a loop and exits 0. No event loop (app:run
-- needs a display and hangs headless). Native objects carry an ffi.gc
-- finalizer, so the loop drops references and nudges collectgarbage.
--
-- Run (matches examples/lua/hello-world.lua):
--   LD_LIBRARY_PATH=. luajit mem_test.lua

local azul = require('azul')

local N = tonumber(os.getenv('AZ_MEMTEST_N')) or 200000

-- 1. The consume-by-value DROP path: App.create consumes the refany +
--    AppConfig. Build it without running (no window), then let it drop.
do
    local data = azul.refany_create({ counter = 5 })
    local app = azul.App.create(data, azul.AppConfig.create())
    app = nil
end
collectgarbage('collect')

-- 2. Leak loop: create/destroy droppable objects N times. ffi.gc frees the
--    native memory on collection, so drop the refs and nudge the collector.
for i = 1, N do
    local cfg = azul.AppConfig.create()
    local dom = azul.Dom.create_body()
    cfg = nil
    dom = nil
    if i % 16384 == 0 then collectgarbage('collect') end
end
collectgarbage('collect')

print(string.format('memtest lua OK (N=%d)', N))
os.exit(0)
