package main

import (
	"fmt"

	azul "azul.rs/ui/go"
)

type counterModel struct {
	Counter int
}

func onClick(model *counterModel, _ *azul.CallbackInfo) azul.Update {
	model.Counter++
	return azul.Update_RefreshDom
}

func layout(model *counterModel, _ *azul.LayoutCallbackInfo) *azul.Dom {
	body := azul.DomCreateBody()
	label := azul.DomCreatePWithText(azul.Str(fmt.Sprintf("%d", model.Counter)))

	button := azul.ButtonCreate(azul.Str("Increase counter"))
	button.OnClick(model, azul.Bind(onClick))

	body.SetCss(azul.Str("p { font-size: 32px; margin: 0; }"))
	body.AddChild(label.Raw())
	body.AddChild(button.Dom().Raw())
	
	return body
}

func main() {
	if err := azul.LoadLibrary("../../target/release/libazul.dylib"); err != nil {
		panic(err)
	}

	data := &counterModel{Counter: 5}
	window := azul.NewWindowCreateOptions(azul.Bind(layout))
	app := azul.AppCreate(data, azul.AppConfigCreate().Raw())
	app.RunWindow(window)
}
