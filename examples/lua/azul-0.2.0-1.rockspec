-- azul-1-1.rockspec
-- Example rockspec for the azul LuaJIT bindings.
--
-- This rockspec installs ONLY the LuaJIT FFI glue (azul.lua).
-- The native shared library (libazul.so / .dylib / azul.dll) must be
-- obtained separately and placed where LuaJIT's `ffi.load("azul")`
-- can locate it (system library path, LD_LIBRARY_PATH on Linux,
-- DYLD_LIBRARY_PATH on macOS, or PATH on Windows).
--
-- LuaJIT-only: PUC Lua does not ship the FFI module.

package = "azul"
version = "0.1.0-1"

source = {
    url = "https://github.com/maps4print/azul/releases/download/v0.1.0/azul-lua-0.1.0.tar.gz",
    -- Replace with actual hash before publishing.
    -- md5 = "...",
}

description = {
    summary  = "LuaJIT FFI bindings for the Azul GUI framework",
    detailed = [[
        Azul is a desktop GUI framework. This rock provides idiomatic
        LuaJIT bindings via the FFI module. The native shared library
        is distributed separately and must be available on the dynamic
        loader's search path at runtime.
    ]],
    homepage = "https://azul.rs",
    license  = "MPL-2.0 OR MIT OR Apache-2.0",
    maintainer = "Azul contributors",
}

dependencies = {
    "lua >= 5.1, < 5.5",
    -- LuaJIT is required (the FFI module is not part of standard Lua).
    -- We can't express "LuaJIT only" in a portable rockspec, so this is
    -- expressed as a Lua-version range and validated at runtime in azul.lua.
}

build = {
    type = "builtin",
    modules = {
        azul = "azul.lua",
    },
}
