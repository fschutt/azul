// examples/go/go.mod
//
// Module manifest for the Go hello-world example.
//
// The `replace` directive lets the example reach the generated `azul`
// package locally without publishing it: drop the generator's output
// (`azul.go`, `types.go`, `functions.go`, `wrappers.go`, plus this
// example's own `azul.h` and the prebuilt native library) into a
// directory named `../azul-go/` next to this file, and the import
// `github.com/azul/azul-go` will resolve there.
//
// In a published / vendored setup, drop the `replace` line and pin a
// real version in `require`.

module github.com/azul/examples/hello-world

go 1.21

require github.com/azul/azul-go v0.0.0-00010101000000-000000000000

replace github.com/azul/azul-go => ../azul-go
