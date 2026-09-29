module azul-app

go 1.21

require azul.rs/ui/go v0.0.0

require github.com/ebitengine/purego v0.10.2 // indirect

// Copy target/codegen/go/ to ./azul-go (and libazul next to the binary).
replace azul.rs/ui/go => ./azul-go
