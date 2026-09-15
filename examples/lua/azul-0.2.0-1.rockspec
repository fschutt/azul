package = "azul"
version = "0.2.0-1"

source = {
    url = "https://azul.rs/ui/release/0.2.0/azul.lua",
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
}

build = {
    type = "builtin",
    modules = {
        azul = "azul.lua",
    },
}
