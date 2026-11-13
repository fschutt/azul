# Copy target/codegen/azul.jl to azul/azul.jl, then: AZUL_LIB=$PWD/libazul.so julia main.jl
include(joinpath(@__DIR__, "azul", "azul.jl"))
using .Azul
include(joinpath(@__DIR__, "styles.jl"))

style_btn_value = style_btn()
println("style_btn: ", style_btn_value.len, " properties")
