# Copy target/codegen/azul.jl to azul/azul.jl, then: AZUL_LIB=$PWD/libazul.so julia main.jl
include(joinpath(@__DIR__, "azul", "azul.jl"))
using .Azul
include(joinpath(@__DIR__, "styles.jl"))

render_ui_value = render_ui()
println("render_ui: ", render_ui_value.len, " properties")
