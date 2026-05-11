// examples/go/main.go
//
// Minimal Go (cgo) smoke test for the Azul C ABI. Confirms that:
//   - the generated `azul.h` is consumable by cgo,
//   - the prebuilt native library loads via `-lazul`,
//   - struct-by-value calls and a basic AzString round-trip succeed.
//
// Go doesn't go through the managed-FFI host-invoker plumbing — cgo
// natively supports struct-by-value calls. Closure-as-callback support
// for the full GUI demo (button click handlers, layout callbacks)
// requires more wrapper-layer machinery; that's separate work, not the
// C ABI surface we exercise here.
//
// Build:
//   CGO_CFLAGS="-I." CGO_LDFLAGS="-L." go build
//   DYLD_LIBRARY_PATH=. ./hello-world

package main

/*
#cgo LDFLAGS: -lazul
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "azul.h"
*/
import "C"

import (
	"fmt"
	"os"
)

func main() {
	// Build a non-empty AzString from a Go []byte. Exercises the
	// C-side `_fromUtf8` API and a struct-by-value return crossing
	// the cgo boundary.
	src := []byte("hello, azul")
	cptr := C.CBytes(src)
	defer C.free(cptr)

	s := C.AzString_fromUtf8((*C.uint8_t)(cptr), C.size_t(len(src)))
	defer C.AzString_delete(&s)

	// Round-trip through clone to confirm the dylib's heap allocator
	// is wired up — _clone allocates a new buffer.
	clone := C.AzString_clone(&s)
	defer C.AzString_delete(&clone)

	if !bool(C.AzString_partialEq(&s, &clone)) {
		fmt.Println("[azul] AzString_clone result not equal to source")
		os.Exit(1)
	}
	fmt.Printf("[azul] AzString round-trip succeeded; len=%d\n", len(src))

	fmt.Println("[azul] cgo init phase completed successfully.")
	fmt.Println("[azul] (Full App.run wiring requires GUI wrapper-layer work")
	fmt.Println("[azul]  separate from the C ABI plumbing exercised here.)")
}
