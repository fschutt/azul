// Idiomatic-Go hello-world counter built entirely through the generated
// azul-go package (no cgo in this file!). Compare with ../main.go, which
// talks raw cgo against azul.h.
//
// Build (from this directory):
//
//	CGO_CFLAGS="-I../../../target/codegen" \
//	CGO_LDFLAGS="-L../../../target/release" \
//	go build
//
// Run:
//
//	DYLD_LIBRARY_PATH=../../../target/release ./hello-world-idiomatic
//
// Callbacks are plain Go functions: the azul package registers them in a
// process-global registry and hands libazul a host-handle callback struct
// (see callbacks.go in the generated package). App data is any Go value,
// wrapped via azul.RefAnyWrap / recovered via azul.RefAnyGet.
package main

import (
	"fmt"
	"os"
	"strings"
	"syscall"

	azul "github.com/azul/azul-go"
)

// libazul's C structs carry Rust NonNull::dangling() sentinels (small
// non-null values like 0x8) in the pointer fields of empty Vecs. Go's
// stack-copy invalid-pointer check aborts on such values when a by-value
// C struct is live on a growing goroutine stack. GODEBUG=invalidptr=0 is
// the documented cgo mitigation ("if you are using cgo and have pointers
// to C memory on the stack"); it cannot be set via //go:debug, so re-exec
// once with it. Delete this guard if you launch with GODEBUG=invalidptr=0.
func init() {
	if strings.Contains(os.Getenv("GODEBUG"), "invalidptr=0") {
		return
	}
	exe, err := os.Executable()
	if err != nil {
		return
	}
	god := os.Getenv("GODEBUG")
	if god != "" {
		god += ","
	}
	god += "invalidptr=0"
	env := make([]string, 0, len(os.Environ())+1)
	for _, kv := range os.Environ() {
		if !strings.HasPrefix(kv, "GODEBUG=") {
			env = append(env, kv)
		}
	}
	env = append(env, "GODEBUG="+god)
	_ = syscall.Exec(exe, os.Args, env)
}

type counterModel struct {
	Counter int
}

func onClick(data *azul.RefAny, _ *azul.CallbackInfo) azul.AzUpdate {
	v, ok := azul.RefAnyGet(data)
	if !ok {
		return azul.AzUpdate_DoNothing
	}
	model, ok := v.(*counterModel)
	if !ok {
		return azul.AzUpdate_DoNothing
	}
	model.Counter++
	return azul.AzUpdate_RefreshDom
}

func layout(data *azul.RefAny, _ *azul.LayoutCallbackInfo) *azul.Dom {
	body := azul.NewDomCreateBody()

	v, ok := azul.RefAnyGet(data)
	if !ok {
		return body
	}
	model, ok := v.(*counterModel)
	if !ok {
		return body
	}

	// Counter label: body > div{font-size:32px} > text("5").
	label := azul.NewDomCreateDiv()
	label.AddChild(azul.NewDomCreateText(azul.Str(fmt.Sprintf("%d", model.Counter))).Raw())

	// Increment button: plain Go function as the click handler.
	button := azul.NewButtonCreate(azul.Str("Increase counter"))
	button.OnClick(data, onClick)

	body.SetCss(azul.Str("div { font-size: 32px; }"))
	body.AddChild(label.Raw())
	body.AddChild(button.Dom())
	return body
}

func main() {
	app := azul.NewAppWithData(&counterModel{Counter: 5}, nil)
	app.RunWindow(azul.NewWindowCreateOptions(layout))
}
