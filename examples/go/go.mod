module hello-world

go 1.21

require azul.rs/ui/go v0.0.0

require github.com/ebitengine/purego v0.10.2 // indirect

// The generated package lives next to main.go, as in the release bundle
// (release/<version>/azul-go) and as scripts/e2e_language_matrix.sh stages
// it from target/codegen/go. After `go get azul.rs/ui/go` this line goes.
replace azul.rs/ui/go => ./azul-go
