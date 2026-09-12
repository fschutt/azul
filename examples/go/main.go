// Idiomatic-Go hello-world counter built entirely through the generated
// azul-go package (no cgo in this file!).
//
// Build (from this directory):
//
//	CGO_CFLAGS="-I../../target/codegen" \
//	CGO_LDFLAGS="-L../../target/release" \
//	go build
//
// Run:
//
//	DYLD_LIBRARY_PATH=../../target/release ./go
//
// Callbacks are plain Go functions: the azul package registers them in a
// process-global registry and hands libazul a host-handle callback struct
// (see callbacks.go in the generated package). App data is any Go value,
// wrapped via azul.RefAnyWrap / recovered via azul.RefAnyGet.
package main

import (
	"fmt"

	azul "github.com/azul/azul-go"
)



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

	// Counter label: body > p > text("5").
	label := azul.NewDomCreatePWithText(azul.Str(fmt.Sprintf("%d", model.Counter)))

	// Increment button: plain Go function as the click handler.
	button := azul.NewButtonCreate(azul.Str("Increase counter"))
	button.OnClick(data, onClick)

	body.SetCss(azul.Str("p { font-size: 32px; margin: 0; }"))
	body.AddChild(label.Raw())
	body.AddChild(button.Dom().Raw())
	return body
}

func main() {
	app := azul.NewAppWithData(&counterModel{Counter: 5}, nil)
	app.RunWindow(azul.NewWindowCreateOptions(layout))
}
