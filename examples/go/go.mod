module azul.rs/examples/hello-world

go 1.21

require azul.rs/ui/go v0.0.0-00010101000000-000000000000

require (
	github.com/ebitengine/purego v0.7.1 // indirect
	golang.org/x/sys v0.7.0 // indirect
)

replace azul.rs/ui/go => ../../target/codegen/go
