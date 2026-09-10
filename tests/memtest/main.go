// Memory test for the azul Go binding. See tests/memtest/README.md.
//
// The harness (scripts/run_memtest.sh) does the measuring: it runs this twice
// with a small and a large AZ_MEMTEST_N and compares peak RSS (RSS that scales
// with N is a LEAK) and watches for any crash (double-free / UAF). This file
// only has to exercise the create/consume/DROP paths in a loop and exit 0.
// No event loop (App.Run needs a display and hangs headless).
//
// Build (from this directory), mirroring examples/go/hello-world-idiomatic:
//
//	CGO_CFLAGS="-I../../target/codegen" \
//	CGO_LDFLAGS="-L../../target/release" \
//	go build
//
// Run:  LD_LIBRARY_PATH=../../target/release ./mem_test
package main

import (
	"fmt"
	"os"
	"strconv"


	azul "github.com/azul/azul-go"
)



type counterModel struct {
	Counter int
}

func main() {
	n := 200000
	if v := os.Getenv("AZ_MEMTEST_N"); v != "" {
		if parsed, err := strconv.Atoi(v); err == nil {
			n = parsed
		}
	}

	// 1. The consume-by-value DROP path: NewAppWithData consumes the
	//    AppConfig (whose nested SystemStyle was one of the bitwise-cloned +
	//    double-freed types). Close() calls AzApp_delete and clears the
	//    finalizer so the drop happens exactly once.
	app := azul.NewAppWithData(&counterModel{Counter: 5}, azul.NewAppConfigCreate())
	app.Close()

	// 2. Leak loop: create/destroy a droppable AppConfig N times. Close()
	//    calls AzAppConfig_delete (dropping the nested SystemStyle) and clears
	//    the SetFinalizer safety net, so each iteration frees deterministically.
	for i := 0; i < n; i++ {
		c := azul.NewAppConfigCreate()
		c.Close()
	}

	fmt.Printf("memtest go OK (N=%d)\n", n)
}
